//! Resource occupancy: one gatherer at a time, the line, and moving on (plan
//! `2026-11-23-harvest-occupancy` D-1 to D-3, B-61).
//!
//! # Responsibility
//! Proves that one worker at a time gathers a mineral field or is inside an extractor; that the
//! next worker in line starts on the tick the gatherer takes; that the line is first come, first
//! served, the lower `EntityId` first on a tie, whatever order the units were made in; that a
//! rival's worker occupies a field too; that gas workers wait and never move on; that workers sent
//! to one field spread over the line while a balanced line stays put; that a worker moves on only
//! to a field with at least two fewer workers assigned, and only within its acquire radius.
//!
//! # How it fits
//! Worlds come from `sim_fixtures` with the shipped catalog; the worker, the hall and the extractor
//! are found by capability, and every time, amount and radius is read from the catalog, never
//! written as a literal. Behaviour is driven by `PlayerCommand`s only; setup may place entities.
//!
//! # Invariants
//! Ticks are simulation ticks; positions are fixed-point world units, compared exactly.

use std::collections::BTreeSet;
use std::error::Error;

use sim_core::catalog::ResourceHarvest;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, MapFile, PlacedFootprint, ResourceNode};
use sim_types::{
    EntityId, HarvesterSpec, PlayerCommand, PlayerId, PlayerSlot, ResourceKind, ResourceNodeId,
    SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);
const SHOWCASE: &str = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");

fn centre(cell: (u32, u32)) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(cell.0) + SimScalar::HALF,
        SimScalar::from_num(cell.1) + SimScalar::HALF,
    )
}

fn node(kind: ResourceKind, cell: (u32, u32), remaining: u32) -> ResourceNode {
    ResourceNode {
        pos: centre(cell),
        kind,
        remaining,
        cell,
        placed: None,
    }
}

/// The worker archetype, the town hall analogue and the gas extractor of `world`'s catalog.
struct Kinds {
    worker: UnitTypeId,
    hall: UnitTypeId,
    extractor: UnitTypeId,
}

fn kinds(world: &SimWorld) -> Result<Kinds, Box<dyn Error>> {
    let find = |test: &dyn Fn(&sim_core::catalog::UnitArchetype) -> bool, what: &'static str| {
        world
            .catalog
            .archetypes()
            .find(|a| test(a))
            .map(|a| a.unit_type_id)
            .ok_or(what)
    };
    Ok(Kinds {
        worker: find(&|a| a.capabilities.harvester.is_some(), "a harvester")?,
        hall: find(
            &|a| a.is_resource_dropoff && a.requires_structures.is_empty(),
            "a depot",
        )?,
        extractor: find(&|a| a.capabilities.is_gas_extractor, "an extractor")?,
    })
}

fn harvest_of(world: &SimWorld, kind: ResourceKind) -> Result<ResourceHarvest, Box<dyn Error>> {
    Ok(*world
        .catalog
        .placement_rules()
        .harvest(kind)
        .ok_or("no harvest row for the kind")?)
}

fn spec(world: &SimWorld) -> Result<HarvesterSpec, Box<dyn Error>> {
    let worker = kinds(world)?.worker;
    Ok(world
        .catalog
        .get(worker)?
        .capabilities
        .harvester
        .ok_or("a harvester spec")?)
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), []);
}

fn harvest_command(
    player: PlayerId,
    workers: Vec<EntityId>,
    target: ResourceNodeId,
) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities: workers,
        order: UnitOrder::Harvest { target },
        queue: false,
    }
}

fn harvest(world: &mut SimWorld, workers: Vec<EntityId>, target: ResourceNodeId) {
    step(world, vec![harvest_command(ME, workers, target)]);
}

/// The node `worker` harvests, if its front order is `Harvest`.
fn target_of(world: &SimWorld, worker: EntityId) -> Option<ResourceNodeId> {
    match world.entities[&worker].order_queue.front() {
        Some(UnitOrder::Harvest { target }) => Some(*target),
        _ => None,
    }
}

fn gathering(world: &SimWorld, worker: EntityId) -> bool {
    world.entities[&worker].task.gather_ticks > 0
}

/// A flat 40 × 40 world with `nodes`, a hall of `ME` at (2, 2) and the bank emptied.
fn flat_world(nodes: Vec<ResourceNode>) -> Result<SimWorld, Box<dyn Error>> {
    let map = GameMap::from_cells(
        40,
        40,
        vec![MapCell::default_ground(CliffLevel(0)); 1600],
        nodes,
        Vec::new(),
    )?;
    let mut world = sim_fixtures::try_world(map, 11)?;
    let hall = kinds(&world)?.hall;
    world.spawn_structure(hall, ME, 2, 2)?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    world.player_resources[PlayerSlot::ALL[0]].gas = 0;
    Ok(world)
}

/// `count` workers of `owner` at `at`.
fn workers(
    world: &mut SimWorld,
    owner: PlayerId,
    at: SimVec2,
    count: usize,
) -> Result<Vec<EntityId>, Box<dyn Error>> {
    let worker = kinds(world)?.worker;
    (0..count)
        .map(|_| Ok(world.spawn_unit(worker, owner, at)?))
        .collect()
}

/// Whether `worker`'s body touches `shape`: at most its collision radius from it, as the map's
/// capsule predicate decides on the Q16 grid.
fn touches(
    world: &SimWorld,
    worker: EntityId,
    shape: &PlacedFootprint,
) -> Result<bool, Box<dyn Error>> {
    let entity = &world.entities[&worker];
    let radius = world.catalog.get(entity.unit_type_id)?.collision_radius;
    Ok(shape.capsule_meets(
        entity.position,
        entity.position,
        radius + SimScalar::from_raw(1 << 16),
    ))
}

/// D-1, D-2: two workers on the one field of a map: while one gathers the other does not, and it
/// stands touching the field counting `wait_ticks`.
#[test]
fn one_worker_gathers_a_field_at_a_time() -> TestResult {
    let mut world = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 1_000)])?;
    let pair = workers(&mut world, ME, centre((9, 4)), 2)?;
    harvest(&mut world, pair.clone(), ResourceNodeId(0));
    let field = world.map.resources()[0]
        .placed
        .ok_or("the field stands")?
        .footprint;
    let mut longest_wait = 0;
    for _ in 0..1_500 {
        step(&mut world, Vec::new());
        let (a, b) = (gathering(&world, pair[0]), gathering(&world, pair[1]));
        assert!(!(a && b), "two gatherers at tick {}", world.current_tick);
        for (gatherer, waiter) in [(pair[0], pair[1]), (pair[1], pair[0])] {
            let wait = world.entities[&waiter].task.wait_ticks;
            if gathering(&world, gatherer) && wait > 0 {
                assert!(
                    touches(&world, waiter, &field)?,
                    "the waiter stands at the field"
                );
                assert_eq!(world.entities[&waiter].cargo, None);
                longest_wait = longest_wait.max(wait);
            }
        }
    }
    assert!(
        longest_wait > 1,
        "the second worker waited in line, counting"
    );
    Ok(())
}

/// D-1 (B-61's restatement, Liquipedia "The next worker can already begin operations during the
/// pause"): whenever a gatherer takes its load while others wait, exactly one waiter starts
/// gathering, for the row's `time_ticks`, on that tick. Three workers on a field near the hall
/// (a trip shorter than three gathers) keep a line standing.
#[test]
fn the_next_worker_starts_on_the_tick_of_the_take() -> TestResult {
    let mut world = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 5_000)])?;
    let mineral = harvest_of(&world, ResourceKind::Mineral)?;
    let crew = workers(&mut world, ME, centre((9, 4)), 3)?;
    harvest(&mut world, crew.clone(), ResourceNodeId(0));
    let mut hand_offs = 0;
    for _ in 0..1_500 {
        let carrying: Vec<bool> = crew
            .iter()
            .map(|&w| world.entities[&w].cargo.is_some())
            .collect();
        let waiting: Vec<bool> = crew
            .iter()
            .map(|&w| world.entities[&w].task.wait_ticks > 0)
            .collect();
        step(&mut world, Vec::new());
        let took = crew
            .iter()
            .zip(&carrying)
            .any(|(&w, &held)| !held && world.entities[&w].cargo.is_some());
        if !took || !waiting.contains(&true) {
            continue;
        }
        let started: Vec<EntityId> = crew
            .iter()
            .zip(&waiting)
            .filter(|(&w, &waited)| waited && gathering(&world, w))
            .map(|(&w, _)| w)
            .collect();
        assert_eq!(
            started.len(),
            1,
            "one waiter starts on the tick of the take"
        );
        assert_eq!(
            world.entities[&started[0]].task.gather_ticks,
            mineral.time_ticks
        );
        hand_offs += 1;
    }
    assert!(hand_offs >= 10, "{hand_offs} hand-offs");
    Ok(())
}

/// The tick each of `line` first gathers after the current tick, stepping at most `limit` ticks.
fn first_gathers(world: &mut SimWorld, line: &[EntityId], limit: u32) -> Vec<Option<u32>> {
    let mut started = vec![None; line.len()];
    for _ in 0..limit {
        step(world, Vec::new());
        for (slot, &worker) in started.iter_mut().zip(line) {
            if slot.is_none() && gathering(world, worker) {
                *slot = Some(world.current_tick);
            }
        }
    }
    started
}

/// D-2, rule 2 (Blizzard: "get in line … and wait their turn"): three workers reaching a field on
/// three different ticks gather in the order they came; two reaching it on one tick gather the
/// lower `EntityId` first.
#[test]
fn the_line_is_first_come_first_served() -> TestResult {
    let mut world = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 1_000)])?;
    // Made in the reverse of the order they are sent, so the arrival order is not the id order.
    let line: Vec<EntityId> = workers(&mut world, ME, centre((9, 4)), 3)?
        .into_iter()
        .rev()
        .collect();
    for &worker in &line {
        harvest(&mut world, vec![worker], ResourceNodeId(0));
        for _ in 0..4 {
            step(&mut world, Vec::new());
        }
    }
    let started = first_gathers(&mut world, &line, 400);
    let ticks: Vec<u32> = started
        .iter()
        .map(|t| t.ok_or("every worker gathers"))
        .collect::<Result<_, _>>()?;
    assert!(ticks[0] < ticks[1] && ticks[1] < ticks[2], "{ticks:?}");

    let mut world = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 1_000)])?;
    let pair = workers(&mut world, ME, centre((9, 4)), 2)?;
    harvest(&mut world, vec![pair[1], pair[0]], ResourceNodeId(0));
    let mut arrived = [None, None];
    for _ in 0..400 {
        step(&mut world, Vec::new());
        for (slot, &worker) in arrived.iter_mut().zip(&pair) {
            let entity = &world.entities[&worker];
            if slot.is_none() && (entity.task.wait_ticks > 0 || entity.task.gather_ticks > 0) {
                *slot = Some(world.current_tick);
            }
        }
        if arrived.iter().all(Option::is_some) {
            break;
        }
    }
    assert_eq!(arrived[0], arrived[1], "both reach the field on one tick");
    assert!(pair[0] < pair[1]);
    assert!(gathering(&world, pair[0]), "the lower id gathers first");
    assert_eq!(world.entities[&pair[1]].task.wait_ticks, 1);
    Ok(())
}

/// The tick, counted from the waiter's order, on which the waiter of a two-worker scene first
/// gathers; `gatherer_first` makes the gatherer the lower id.
fn waiter_start(gatherer_first: bool) -> Result<u32, Box<dyn Error>> {
    let mut world = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 1_000)])?;
    let worker = kinds(&world)?.worker;
    let (gatherer, waiter) = if gatherer_first {
        let g = world.spawn_unit(worker, ME, centre((9, 4)))?;
        (g, world.spawn_unit(worker, ME, centre((9, 4)))?)
    } else {
        let w = world.spawn_unit(worker, ME, centre((9, 4)))?;
        (world.spawn_unit(worker, ME, centre((9, 4)))?, w)
    };
    harvest(&mut world, vec![gatherer], ResourceNodeId(0));
    for _ in 0..10 {
        step(&mut world, Vec::new());
    }
    harvest(&mut world, vec![waiter], ResourceNodeId(0));
    let ordered = world.current_tick;
    for _ in 0..400 {
        step(&mut world, Vec::new());
        if gathering(&world, waiter) {
            assert!(
                world.entities[&gatherer].cargo.is_some() && !gathering(&world, gatherer),
                "the waiter starts on the gatherer's take"
            );
            return Ok(world.current_tick - ordered);
        }
    }
    Err("the waiter never gathered".into())
}

/// D-2, T5: the grant waits for the resolution after every task step, so the waiter starts on the
/// tick of the gatherer's take whether the gatherer is stepped before it or after it.
#[test]
fn the_grant_does_not_depend_on_entity_order() -> TestResult {
    assert_eq!(waiter_start(true)?, waiter_start(false)?);
    Ok(())
}

/// Rule 7 (Liquipedia, Mining Minerals: an opponent's worker "prevents the enemy's worker from
/// mining as the field would then be occupied"): a rival's gatherer occupies the field.
#[test]
fn an_enemy_worker_occupies_a_field() -> TestResult {
    let mut world = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 1_000)])?;
    let hall = kinds(&world)?.hall;
    world.spawn_structure(hall, RIVAL, 20, 2)?;
    let mine = workers(&mut world, ME, centre((9, 4)), 1)?[0];
    let theirs = workers(&mut world, RIVAL, centre((15, 4)), 1)?[0];
    step(
        &mut world,
        vec![
            harvest_command(ME, vec![mine], ResourceNodeId(0)),
            harvest_command(RIVAL, vec![theirs], ResourceNodeId(0)),
        ],
    );
    let mut blocked = 0;
    for _ in 0..1_500 {
        step(&mut world, Vec::new());
        assert!(!(gathering(&world, mine) && gathering(&world, theirs)));
        if gathering(&world, theirs) && world.entities[&mine].task.wait_ticks > 0 {
            blocked += 1;
        }
    }
    assert!(blocked > 0, "the rival's gatherer kept my worker waiting");
    Ok(())
}

/// A showcase world with slot 0's hall at its start; the bank emptied. Returns the world and the
/// start cell.
fn showcase() -> Result<(SimWorld, (u32, u32)), Box<dyn Error>> {
    let doc = MapFile::from_ron_str(SHOWCASE)?.decode()?;
    let start = doc
        .start_locations
        .iter()
        .find(|s| s.player_slot == 0)
        .ok_or("slot 0")?
        .cell;
    let mut world = sim_fixtures::try_world(doc.to_game_map()?, 7)?;
    let hall = kinds(&world)?.hall;
    world.spawn_structure(hall, ME, start.0, start.1)?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    world.player_resources[PlayerSlot::ALL[0]].gas = 0;
    Ok((world, start))
}

fn showcase_node(
    world: &SimWorld,
    kind: ResourceKind,
    cell: (u32, u32),
) -> Result<ResourceNodeId, Box<dyn Error>> {
    Ok((0_u32..)
        .zip(world.map.resources())
        .find(|(_, n)| n.kind == kind && n.cell == cell)
        .map(|(i, _)| ResourceNodeId(i))
        .ok_or("no such node on the showcase")?)
}

/// The showcase's slot-0 mineral fields, in node order.
fn slot0_fields(world: &SimWorld) -> Vec<ResourceNodeId> {
    (0_u32..)
        .zip(world.map.resources())
        .filter(|(_, n)| n.kind == ResourceKind::Mineral && n.cell.0 < 32)
        .map(|(i, _)| ResourceNodeId(i))
        .collect()
}

/// D-3, rule 4 (Blizzard: "more than one worker queued up waiting for the geyser"): three workers
/// on showcase slot 0's near geyser with a second extractor free on the corner geyser never move
/// to it; one at a time is inside, and once saturated the geyser is taken every `time_ticks`.
#[test]
fn one_worker_at_a_time_in_an_extractor_and_gas_workers_wait() -> TestResult {
    let (mut world, start) = showcase()?;
    let gas = harvest_of(&world, ResourceKind::Gas)?;
    let extractor = kinds(&world)?.extractor;
    let near = showcase_node(&world, ResourceKind::Gas, (10, 1))?;
    let corner = showcase_node(&world, ResourceKind::Gas, (1, 1))?;
    world.spawn_structure(extractor, ME, 9, 0)?;
    world.spawn_structure(extractor, ME, 0, 0)?;
    let crew = workers(
        &mut world,
        ME,
        SimVec2::new(
            SimScalar::from_num(start.0 + 6),
            SimScalar::from_num(start.1 + 6),
        ),
        3,
    )?;
    harvest(&mut world, crew.clone(), near);
    let mut takes = Vec::new();
    let mut last = world.map.resources()[near.0 as usize].remaining;
    for tick in 0..3_750_u32 {
        step(&mut world, Vec::new());
        let inside = crew.iter().filter(|&&w| gathering(&world, w)).count();
        assert!(inside <= 1, "{inside} workers inside at tick {tick}");
        for &worker in &crew {
            let target = target_of(&world, worker);
            assert_eq!(target, Some(near), "a gas worker moved on");
            assert_ne!(target, Some(corner));
        }
        let now = world.map.resources()[near.0 as usize].remaining;
        if now < last && tick >= 750 {
            takes.push(tick);
        }
        last = now;
    }
    assert!(takes.len() > 50, "{} takes", takes.len());
    for pair in takes.windows(2) {
        assert_eq!(pair[1] - pair[0], u32::from(gas.time_ticks), "{takes:?}");
    }
    Ok(())
}

/// Spawns `k` workers per field of `fields` at the hall's edge facing that field (`start` is the
/// hall's corner cell) and orders each group to its field on one tick; returns the groups.
fn send_line(
    world: &mut SimWorld,
    start: (u32, u32),
    fields: &[ResourceNodeId],
    k: usize,
) -> Result<Vec<Vec<EntityId>>, Box<dyn Error>> {
    let hall = SimVec2::new(
        SimScalar::from_num(start.0) + SimScalar::from_num(5) / SimScalar::from_num(2),
        SimScalar::from_num(start.1) + SimScalar::from_num(5) / SimScalar::from_num(2),
    );
    let mut groups = Vec::new();
    let mut commands = Vec::new();
    for (lane, &field) in fields.iter().enumerate() {
        let toward = world.map.resources()[field.0 as usize].pos;
        let direction = (toward - hall).normalize_or_zero();
        // Lanes 3.2, 3.6 and 4.0 out from the hall's centre, workers 0.3 apart across the lane.
        let out = SimScalar::from_num(16 + 2 * (u32::try_from(lane)? % 3)) / SimScalar::from_num(5);
        let edge = hall + direction * out;
        let across = SimVec2::new(-direction.y, direction.x);
        let mut group = Vec::new();
        for i in 0..k {
            let offset = SimScalar::from_num(3 * u32::try_from(i)?) / SimScalar::from_num(10);
            group.push(world.spawn_unit(kinds(world)?.worker, ME, edge + across * offset)?);
        }
        commands.push(harvest_command(ME, group.clone(), field));
        groups.push(group);
    }
    step(world, commands);
    Ok(groups)
}

/// A-3 (Liquipedia, Worker split: "sending your initial 6 workers to a single patch will trigger
/// the AI to automatically send the 5 remaining workers to other patches"): six workers sent to
/// one showcase field end on six different fields.
#[test]
fn workers_sent_to_one_field_spread_over_the_line() -> TestResult {
    let (mut world, start) = showcase()?;
    let fields = slot0_fields(&world);
    let first = *fields.first().ok_or("a field")?;
    let group = send_line(&mut world, start, &[first], 6)?.remove(0);
    for _ in 0..1_500 {
        step(&mut world, Vec::new());
    }
    let targets: BTreeSet<ResourceNodeId> = group
        .iter()
        .map(|&w| target_of(&world, w).ok_or("still harvesting"))
        .collect::<Result<_, _>>()?;
    assert_eq!(targets.len(), 6, "{targets:?}");
    assert!(targets.iter().all(|t| fields.contains(t)));
    Ok(())
}

/// A-2 (Liquipedia, Mining Minerals: "workers will self-balance"): with two, then three, workers
/// on every showcase field, one gathering at a time, no worker changes field after the warm-up.
#[test]
fn a_balanced_line_stays_put() -> TestResult {
    for k in [2, 3] {
        let (mut world, start) = showcase()?;
        let fields = slot0_fields(&world);
        let groups = send_line(&mut world, start, &fields, k)?;
        for _ in 0..1_500 {
            step(&mut world, Vec::new());
        }
        let settled: Vec<Option<ResourceNodeId>> = groups
            .iter()
            .flatten()
            .map(|&w| target_of(&world, w))
            .collect();
        for _ in 0..3_000 {
            step(&mut world, Vec::new());
            for &field in &fields {
                let gatherers = groups
                    .iter()
                    .flatten()
                    .filter(|&&w| target_of(&world, w) == Some(field) && gathering(&world, w))
                    .count();
                assert!(gatherers <= 1, "k = {k}: {gatherers} gather {field:?}");
            }
            let now: Vec<Option<ResourceNodeId>> = groups
                .iter()
                .flatten()
                .map(|&w| target_of(&world, w))
                .collect();
            assert_eq!(
                now, settled,
                "k = {k}: a worker changed field at {}",
                world.current_tick
            );
        }
    }
    Ok(())
}

/// The workers of each of two fields 3 cells apart after 2,000 ticks, with `a` workers sent to
/// the first and `b` to the second.
fn two_fields(a: usize, b: usize) -> Result<(usize, usize, usize), Box<dyn Error>> {
    let mut world = flat_world(vec![
        node(ResourceKind::Mineral, (12, 4), 5_000),
        node(ResourceKind::Mineral, (12, 7), 5_000),
    ])?;
    let first = workers(&mut world, ME, centre((9, 4)), a)?;
    let second = workers(&mut world, ME, centre((9, 7)), b)?;
    step(
        &mut world,
        vec![
            harvest_command(ME, first.clone(), ResourceNodeId(0)),
            harvest_command(ME, second.clone(), ResourceNodeId(1)),
        ],
    );
    let mut moves = 0;
    let all: Vec<EntityId> = first.iter().chain(&second).copied().collect();
    let mut last: Vec<Option<ResourceNodeId>> = all.iter().map(|&w| target_of(&world, w)).collect();
    for _ in 0..2_000 {
        step(&mut world, Vec::new());
        let now: Vec<Option<ResourceNodeId>> = all.iter().map(|&w| target_of(&world, w)).collect();
        moves += now.iter().zip(&last).filter(|(n, l)| n != l).count();
        last = now;
    }
    let on = |node: u32| {
        last.iter()
            .filter(|t| **t == Some(ResourceNodeId(node)))
            .count()
    };
    Ok((on(0), on(1), moves))
}

/// D-3: a worker moves on only to a field with at least two fewer workers assigned. Fields with
/// (3, 2) workers never exchange one (Liquipedia: a third worker stays "unless they are also
/// over-saturated"); (3, 1) gives one worker to the second field and then stays (2, 2).
#[test]
fn a_worker_moves_only_to_a_field_two_fewer() -> TestResult {
    assert_eq!(two_fields(3, 2)?, (3, 2, 0));
    assert_eq!(two_fields(3, 1)?, (2, 2, 1));
    Ok(())
}

/// D-3, D-4: a free field farther than the harvester's `acquire_radius` from the waiting workers
/// is never chosen, however many fewer workers it has.
#[test]
fn moving_on_respects_the_acquire_radius() -> TestResult {
    let probe = flat_world(Vec::new())?;
    let radius = spec(&probe)?.acquire_radius;
    let beyond = u32::try_from((radius.raw() >> 32) + 1)? + 4;
    let mut world = flat_world(vec![
        node(ResourceKind::Mineral, (12, 4), 5_000),
        node(ResourceKind::Mineral, (12 + beyond, 4), 5_000),
    ])?;
    let crew = workers(&mut world, ME, centre((9, 4)), 4)?;
    harvest(&mut world, crew.clone(), ResourceNodeId(0));
    let mut waited = false;
    for _ in 0..1_500 {
        step(&mut world, Vec::new());
        for &worker in &crew {
            assert_eq!(
                target_of(&world, worker),
                Some(ResourceNodeId(0)),
                "moved beyond the radius"
            );
            waited |= world.entities[&worker].task.wait_ticks > 0;
        }
    }
    assert!(waited, "the workers waited in line instead");
    Ok(())
}
