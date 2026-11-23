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
use sim_map::{CliffLevel, GameMap, MapCell, ResourceNode};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, PlayerSlot, ResourceKind, ResourceNodeId, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);

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

/// One hand-off seen in a line: the tick of a take with a worker waiting, and the tick on which
/// the first of those waiters started gathering, with its `gather_ticks` then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandOff {
    take: u32,
    start: u32,
    gather_ticks: u16,
    /// How many of the waiters started on that tick.
    starters: usize,
}

/// Steps `world` `ticks` times and records every hand-off in `crew`'s line: each take by one of
/// `crew` while another of `crew` waits (touching the node, `wait_ticks` above 0), and the first
/// tick on which one of those waiters gathers.
fn hand_offs(world: &mut SimWorld, crew: &[EntityId], ticks: u32) -> Vec<HandOff> {
    let mut seen = Vec::new();
    let mut pending: Option<(u32, Vec<EntityId>)> = None;
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
        let took = crew
            .iter()
            .zip(&carrying)
            .any(|(w, &held)| !held && world.entities[w].cargo.is_some());
        if took && !waiting.is_empty() && pending.is_none() {
            pending = Some((world.current_tick, waiting));
        }
        if let Some((take, waiters)) = &pending {
            let starters: Vec<EntityId> = waiters
                .iter()
                .copied()
                .filter(|w| gathering(world, *w))
                .collect();
            if let Some(first) = starters.first() {
                seen.push(HandOff {
                    take: *take,
                    start: world.current_tick,
                    gather_ticks: world.entities[first].task.gather_ticks,
                    starters: starters.len(),
                });
                pending = None;
            }
        }
    }
    seen
}

/// Three workers sent to the one node of `kind` in `world`, which keep a line standing (a trip
/// shorter than three gathers): every hand-off over 1,500 ticks.
fn crew_of_three(world: &mut SimWorld) -> Result<(Vec<EntityId>, Vec<HandOff>), Box<dyn Error>> {
    let crew = workers(world, ME, centre((9, 4)), 3)?;
    step(
        world,
        vec![harvest_command(ME, crew.clone(), ResourceNodeId(0))],
    );
    let seen = hand_offs(world, &crew, 1_500);
    Ok((crew, seen))
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
        assert!(seen.len() >= 10, "{kind:?}: {} hand-offs", seen.len());
        for hand_off in &seen {
            assert_eq!(
                *hand_off,
                HandOff {
                    take: hand_off.take,
                    start: hand_off.take,
                    gather_ticks: harvest.time_ticks,
                    starters: 1,
                },
                "{kind:?}"
            );
        }
    }
    Ok(())
}
