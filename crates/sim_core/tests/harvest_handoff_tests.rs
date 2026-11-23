//! The hand-off between two gatherers (plan `2026-11-23-harvest-handoff` D-1 to D-3, B-162).
//!
//! # Responsibility
//! Proves when the next worker in a resource's line starts: `handoff_ticks` of the node's kind
//! after the previous worker's take, the node counting as occupied until then. A hand-off of 0
//! is the occupancy plan's rule, the waiter starting on the tick of the take.
//!
//! # How it fits
//! Worlds come from `sim_fixtures`' shipped catalogs; a test that needs another hand-off parses
//! a copy of the shipped `placement_rules.ron` with its `handoff_ticks` replaced, so the catalog
//! is the shipped one in everything else. The worker, the hall and the extractor are found by
//! capability, and every time, amount and hand-off is read from the catalog, never written as a
//! literal. Behaviour is driven by `PlayerCommand`s only; setup may place entities.
//!
//! # Invariants
//! Ticks are simulation ticks: a tick's number is `SimWorld::current_tick` after its step.
//! Positions are fixed-point world units.

use std::error::Error;

use sim_core::catalog::{PlacementRules, ResourceHarvest};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, MapFile, ResourceNode};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, PlayerSlot, ResourceKind, ResourceNodeId, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
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

/// The shipped `placement_rules.ron` with every row's `handoff_ticks` set to `ticks`.
fn rules_with_handoff(ticks: u16) -> Result<PlacementRules, Box<dyn Error>> {
    const FIELD: &str = "handoff_ticks:";
    let shipped = sim_fixtures::PLACEMENT_RULES_RON;
    let mut edited = String::with_capacity(shipped.len());
    let mut rest = shipped;
    let mut rows = 0;
    while let Some(at) = rest.find(FIELD) {
        let after = &rest[at + FIELD.len()..];
        let value = after.trim_start();
        let digits = value.len() - value.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return Err("a handoff_ticks field without a number".into());
        }
        edited.push_str(&rest[..at]);
        edited.push_str(FIELD);
        edited.push(' ');
        edited.push_str(&ticks.to_string());
        rest = &value[digits..];
        rows += 1;
    }
    edited.push_str(rest);
    if rows != 2 {
        return Err(format!("{rows} handoff_ticks fields, not one per kind").into());
    }
    Ok(PlacementRules::from_ron_str(&edited)?)
}

/// A flat 40 × 40 world with `nodes`, a hall of `ME` at (2, 2) and the bank emptied; with
/// `handoff` set, every kind's hand-off is that many ticks, the shipped catalog otherwise.
fn flat_world(nodes: Vec<ResourceNode>, handoff: Option<u16>) -> Result<SimWorld, Box<dyn Error>> {
    let map = GameMap::from_cells(
        40,
        40,
        vec![MapCell::default_ground(CliffLevel(0)); 1600],
        nodes,
        Vec::new(),
    )?;
    let mut catalogs = sim_fixtures::shipped_catalogs();
    if let Some(ticks) = handoff {
        catalogs
            .units
            .set_placement_rules(rules_with_handoff(ticks)?);
    }
    let mut world = SimWorld::with_catalogs(map, 11, catalogs.units, catalogs.upgrades)?;
    let hall = kinds(&world)?.hall;
    world.spawn_structure(hall, ME, 2, 2)?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    world.player_resources[PlayerSlot::ALL[0]].gas = 0;
    Ok(world)
}

/// The mineral field of [`flat_world`] the crews gather from.
const FIELD: (u32, u32) = (12, 4);
/// The geyser of [`flat_world`] the gas crews gather from; its extractor stands on the 3 × 3
/// centred on it.
const GEYSER: (u32, u32) = (11, 5);

/// A flat world with one node of `kind` (and, for gas, a completed extractor of `ME` over it).
fn lone_node(kind: ResourceKind, handoff: Option<u16>) -> Result<SimWorld, Box<dyn Error>> {
    let cell = match kind {
        ResourceKind::Mineral => FIELD,
        ResourceKind::Gas => GEYSER,
    };
    let mut world = flat_world(vec![node(kind, cell, 20_000)], handoff)?;
    if kind == ResourceKind::Gas {
        let extractor = kinds(&world)?.extractor;
        world.spawn_structure(extractor, ME, cell.0 - 1, cell.1 - 1)?;
    }
    Ok(world)
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

fn gathering(world: &SimWorld, worker: EntityId) -> bool {
    world.entities[&worker].task.gather_ticks > 0
}

/// One hand-off seen in a line: a take with a worker waiting, and the first of those waiters
/// to start gathering afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandOff {
    /// The tick of the take.
    take: u32,
    /// The worker that took.
    taker: EntityId,
    /// The tick on which a waiter first gathered after it.
    start: u32,
    /// That waiter.
    starter: EntityId,
    /// Its `gather_ticks` on that tick.
    gather_ticks: u16,
    /// How many of the waiters started on that tick.
    starters: usize,
}

/// Steps `world` `ticks` times and records every hand-off in `crew`'s line: each take by one of
/// `crew` while another of `crew` waits (touching the node, `wait_ticks` above 0), and the first
/// tick on which one of those waiters gathers.
fn hand_offs(world: &mut SimWorld, crew: &[EntityId], ticks: u32) -> Vec<HandOff> {
    let mut seen = Vec::new();
    let mut pending: Option<(u32, EntityId, Vec<EntityId>)> = None;
    for _ in 0..ticks {
        let carrying: Vec<bool> = crew
            .iter()
            .map(|w| world.entities[w].cargo.is_some())
            .collect();
        let waiting: Vec<EntityId> = crew
            .iter()
            .copied()
            .filter(|w| world.entities[w].task.wait_ticks > 0 && !gathering(world, *w))
            .collect();
        step(world, Vec::new());
        let taker = crew
            .iter()
            .zip(&carrying)
            .find(|(w, &held)| !held && world.entities[*w].cargo.is_some())
            .map(|(w, _)| *w);
        if let Some(taker) = taker {
            if !waiting.is_empty() && pending.is_none() {
                pending = Some((world.current_tick, taker, waiting));
            }
        }
        if let Some((take, taker, waiters)) = &pending {
            let starters: Vec<EntityId> = waiters
                .iter()
                .copied()
                .filter(|w| gathering(world, *w))
                .collect();
            if let Some(&first) = starters.first() {
                seen.push(HandOff {
                    take: *take,
                    taker: *taker,
                    start: world.current_tick,
                    starter: first,
                    gather_ticks: world.entities[&first].task.gather_ticks,
                    starters: starters.len(),
                });
                pending = None;
            }
        }
    }
    seen
}

/// Three workers sent to the one node of `world`, which keep a line standing (a trip shorter
/// than three gathers and hand-offs): every hand-off over 1,500 ticks.
fn crew_of_three(world: &mut SimWorld) -> Result<(Vec<EntityId>, Vec<HandOff>), Box<dyn Error>> {
    let crew = workers(world, ME, centre((9, 4)), 3)?;
    step(
        world,
        vec![harvest_command(ME, crew.clone(), ResourceNodeId(0))],
    );
    let seen = hand_offs(world, &crew, 1_500);
    Ok((crew, seen))
}

/// Asserts that every hand-off of `seen` starts exactly one waiter, `handoff` ticks after the
/// take, gathering for `time` ticks.
fn assert_every_handoff(seen: &[HandOff], handoff: u16, time: u16, what: &str) {
    assert!(seen.len() >= 10, "{what}: {} hand-offs", seen.len());
    for hand_off in seen {
        assert_eq!(
            (
                hand_off.start - hand_off.take,
                hand_off.gather_ticks,
                hand_off.starters
            ),
            (u32::from(handoff), time, 1),
            "{what}: {hand_off:?}"
        );
    }
}

/// D-1, §15 (b): with `handoff_ticks: 0` on every row the rule is the occupancy plan's: on every
/// hand-off, of a mineral field and of an extractor, exactly one waiter starts gathering, for
/// the row's `time_ticks`, on the tick of the take.
#[test]
fn a_zero_handoff_is_todays_rule() -> TestResult {
    for kind in [ResourceKind::Mineral, ResourceKind::Gas] {
        let mut world = lone_node(kind, Some(0))?;
        let harvest = harvest_of(&world, kind)?;
        assert_eq!(harvest.handoff_ticks, 0);
        let (_, seen) = crew_of_three(&mut world)?;
        assert_every_handoff(&seen, 0, harvest.time_ticks, &format!("{kind:?}"));
    }
    Ok(())
}

/// D-1, §2.4 rule 1 (A-3): with the shipped rows, on every hand-off of a lone mineral field and of
/// a lone extractor with three workers, the waiter's `gather_ticks` stays 0 from the take's tick
/// `t` to `t + h − 1` and becomes the row's `time_ticks` on `t + h`, `h` the row's
/// `handoff_ticks`. Exactly one waiter starts. The shipped rows hand off after the take (D-2),
/// so this is not the zero hand-off's rule.
#[test]
fn the_next_worker_starts_handoff_ticks_after_the_take() -> TestResult {
    for kind in [ResourceKind::Mineral, ResourceKind::Gas] {
        let mut world = lone_node(kind, None)?;
        let harvest = harvest_of(&world, kind)?;
        assert!(
            harvest.handoff_ticks > 0,
            "{kind:?}: the shipped row hands off after the take"
        );
        let (_, seen) = crew_of_three(&mut world)?;
        assert_every_handoff(
            &seen,
            harvest.handoff_ticks,
            harvest.time_ticks,
            &format!("{kind:?}"),
        );
    }
    Ok(())
}

/// The tick on which `worker` first gathers within `limit` ticks, if it does.
fn first_gather(world: &mut SimWorld, worker: EntityId, limit: u32) -> Option<u32> {
    for _ in 0..limit {
        step(world, Vec::new());
        if gathering(world, worker) {
            return Some(world.current_tick);
        }
    }
    None
}

/// D-1 ("a worker arriving during the hand-off waits"): the release is the node's, not a
/// waiter's. A second worker placed touching the field on the tick after the first worker's take
/// claims it on that tick and is granted it on the take's tick plus the row's `handoff_ticks`,
/// not before. The shipped mineral hand-off is at least 2 ticks, so the tick after the take lies
/// inside it.
#[test]
fn a_worker_arriving_during_the_handoff_waits_for_it() -> TestResult {
    let mut world = lone_node(ResourceKind::Mineral, None)?;
    let mineral = harvest_of(&world, ResourceKind::Mineral)?;
    assert!(
        mineral.handoff_ticks >= 2,
        "the tick after the take lies inside the shipped hand-off"
    );
    let first = workers(&mut world, ME, centre((9, 4)), 1)?[0];
    step(
        &mut world,
        vec![harvest_command(ME, vec![first], ResourceNodeId(0))],
    );
    let mut take = None;
    for _ in 0..600 {
        step(&mut world, Vec::new());
        if world.entities[&first].cargo.is_some() {
            take = Some(world.current_tick);
            break;
        }
    }
    let take = take.ok_or("the first worker never took")?;
    // The field's 2 × 1 box spans x 12 to 14 and y 4 to 5: the second worker's body touches its
    // east face, away from the first worker, which came from the west.
    let radius = world.catalog.get(kinds(&world)?.worker)?.collision_radius;
    let east = SimVec2::new(SimScalar::from_num(14) + radius, SimScalar::from_num(4.5));
    let second = workers(&mut world, ME, east, 1)?[0];
    step(
        &mut world,
        vec![harvest_command(ME, vec![second], ResourceNodeId(0))],
    );
    assert_eq!(world.current_tick, take + 1);
    assert_eq!(
        world.entities[&second].task.wait_ticks, 1,
        "the second worker touches the field and claims it on the tick after the take"
    );
    assert!(!gathering(&world, second));
    let start = first_gather(&mut world, second, 50).ok_or("the second worker never gathered")?;
    assert_eq!(start, take + u32::from(mineral.handoff_ticks));
    Ok(())
}

/// Every tick on which `worker`, alone at the one node of `world`, takes a load within `ticks`;
/// asserts on every tick that it never waits (`wait_ticks` stays 0).
fn lone_takes(world: &mut SimWorld, ticks: u32) -> Result<Vec<u32>, Box<dyn Error>> {
    let worker = workers(world, ME, centre((9, 4)), 1)?[0];
    step(
        world,
        vec![harvest_command(ME, vec![worker], ResourceNodeId(0))],
    );
    let mut takes = Vec::new();
    for _ in 0..ticks {
        let held = world.entities[&worker].cargo.is_some();
        step(world, Vec::new());
        assert_eq!(
            world.entities[&worker].task.wait_ticks, 0,
            "a lone worker waited at tick {}",
            world.current_tick
        );
        if !held && world.entities[&worker].cargo.is_some() {
            takes.push(world.current_tick);
        }
    }
    Ok(takes)
}

/// D-1, §2.4 rule 4 (A-4): a worker reaching a free node starts on the tick it touches it, so a
/// lone worker never waits, and its takes fall on the same ticks as under a zero hand-off, on a
/// mineral field and at an extractor: its trip is unchanged.
#[test]
fn a_worker_arriving_at_a_free_node_starts_at_once() -> TestResult {
    for kind in [ResourceKind::Mineral, ResourceKind::Gas] {
        let mut shipped = lone_node(kind, None)?;
        assert!(harvest_of(&shipped, kind)?.handoff_ticks > 0);
        let with_handoff = lone_takes(&mut shipped, 1_500)?;
        let without = lone_takes(&mut lone_node(kind, Some(0))?, 1_500)?;
        assert!(with_handoff.len() >= 5, "{kind:?}: {with_handoff:?}");
        assert_eq!(with_handoff, without, "{kind:?}");
    }
    Ok(())
}

/// D-1 (occupancy is shared by every player, occupancy plan D-1): the release tick is the node's,
/// whoever took. Two workers of each of two players on one field: every hand-off starts the row's
/// `handoff_ticks` after the take, and among them a rival's waiter starts after my worker's take.
#[test]
fn the_handoff_is_shared_by_every_player() -> TestResult {
    let mut world = lone_node(ResourceKind::Mineral, None)?;
    let mineral = harvest_of(&world, ResourceKind::Mineral)?;
    assert!(mineral.handoff_ticks > 0);
    let hall = kinds(&world)?.hall;
    world.spawn_structure(hall, RIVAL, 20, 2)?;
    let mine = workers(&mut world, ME, centre((9, 4)), 2)?;
    let theirs = workers(&mut world, RIVAL, centre((17, 4)), 2)?;
    step(
        &mut world,
        vec![
            harvest_command(ME, mine.clone(), ResourceNodeId(0)),
            harvest_command(RIVAL, theirs.clone(), ResourceNodeId(0)),
        ],
    );
    let crew: Vec<EntityId> = mine.iter().chain(&theirs).copied().collect();
    let seen = hand_offs(&mut world, &crew, 1_500);
    assert_every_handoff(
        &seen,
        mineral.handoff_ticks,
        mineral.time_ticks,
        "two players",
    );
    assert!(
        seen.iter()
            .any(|h| mine.contains(&h.taker) && theirs.contains(&h.starter)),
        "a rival's worker waited through my worker's hand-off: {seen:?}"
    );
    Ok(())
}

/// A showcase world with slot 0's hall at its start, an extractor over the near geyser (10, 1)
/// and the bank emptied, and three workers at the hall's edge: the world, the workers and the
/// geyser.
fn showcase_gas_crew() -> Result<(SimWorld, Vec<EntityId>, ResourceNodeId), Box<dyn Error>> {
    let doc = MapFile::from_ron_str(SHOWCASE)?.decode()?;
    let start = doc
        .start_locations
        .iter()
        .find(|s| s.player_slot == 0)
        .ok_or("slot 0")?
        .cell;
    let mut world = sim_fixtures::try_world(doc.to_game_map()?, 7)?;
    let found = kinds(&world)?;
    world.spawn_structure(found.hall, ME, start.0, start.1)?;
    world.spawn_structure(found.extractor, ME, 9, 0)?;
    world.player_resources[PlayerSlot::ALL[0]].gas = 0;
    let geyser = (0_u32..)
        .zip(world.map.resources())
        .find(|(_, n)| n.kind == ResourceKind::Gas && n.cell == (10, 1))
        .map(|(i, _)| ResourceNodeId(i))
        .ok_or("the near geyser")?;
    let at = SimVec2::new(
        SimScalar::from_num(start.0 + 6),
        SimScalar::from_num(start.1 + 6),
    );
    let crew = workers(&mut world, ME, at, 3)?;
    Ok((world, crew, geyser))
}

/// The ticks of every take from `node` in a `window` after `warm` ticks.
fn takes_of(world: &mut SimWorld, node: ResourceNodeId, warm: u32, window: u32) -> Vec<u32> {
    for _ in 0..warm {
        step(world, Vec::new());
    }
    let mut last = world.map.resources()[node.0 as usize].remaining;
    let mut takes = Vec::new();
    for _ in 0..window {
        step(world, Vec::new());
        let now = world.map.resources()[node.0 as usize].remaining;
        if now < last {
            takes.push(world.current_tick);
        }
        last = now;
    }
    takes
}

/// §6.1 (P-4, A-1: a saturated node is exact): three workers on the showcase's near geyser keep
/// it saturated (gas workers wait and never move on), and so do three on the lone field of a flat
/// map (a showcase field's extra workers would spread over the line; the line at three a field is
/// `harvest_edge_tests::saturation_follows_the_derivation`). Each node is then taken every
/// `time_ticks + handoff_ticks` of its row, read from the catalog.
#[test]
fn a_saturated_node_takes_every_time_plus_handoff_ticks() -> TestResult {
    for kind in [ResourceKind::Gas, ResourceKind::Mineral] {
        let (mut world, crew, node, warm) = if kind == ResourceKind::Gas {
            let (world, crew, geyser) = showcase_gas_crew()?;
            (world, crew, geyser, 750)
        } else {
            let mut world = lone_node(kind, None)?;
            let crew = workers(&mut world, ME, centre((9, 4)), 3)?;
            (world, crew, ResourceNodeId(0), 1_500)
        };
        let harvest = harvest_of(&world, kind)?;
        assert!(harvest.handoff_ticks > 0);
        step(&mut world, vec![harvest_command(ME, crew, node)]);
        let takes = takes_of(&mut world, node, warm, 3_000);
        assert!(takes.len() > 50, "{kind:?}: {} takes", takes.len());
        let period = u32::from(harvest.time_ticks) + u32::from(harvest.handoff_ticks);
        for pair in takes.windows(2) {
            assert_eq!(pair[1] - pair[0], period, "{kind:?}: {takes:?}");
        }
    }
    Ok(())
}
