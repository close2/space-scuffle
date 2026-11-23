//! Harvest timing per resource (plan `2026-11-22-harvest-timing`, B-150 and B-151).
//!
//! # Responsibility
//! Proves the rule of D-1 and D-2: a gather lasts its resource row's `harvest_time_ticks` and
//! yields its `harvest_amount`; a worker that has taken a load waits at the node, holding it, for
//! the row's `return_delay_ticks` before it walks back, and a row without a delay sends it back at
//! once; the node is drawn down at the take, before the wait, and a field emptied by the take
//! leaves the map then; a new order during the wait starts when the wait would have ended and the
//! cargo stays (plan `2026-11-23-harvest-occupancy` D-5, B-160), while a queued one changes nothing;
//! a trip is its two walking legs plus the row's two times; and a near geyser of the showcase
//! earns the game's rate (§6.2).
//!
//! # How it fits
//! Worlds come from `sim_fixtures` with the shipped catalog; the worker, the hall and the
//! extractor are found by capability, and every time and amount is read from the catalog's
//! resource rows, never written as a literal. Behaviour is driven by `PlayerCommand`s only;
//! setup may place entities.
//!
//! # Invariants
//! Ticks are simulation ticks (25 a Faster second); rates are resource units per worker per
//! minute (1,500 ticks), compared by exact integer cross-multiplication.

use std::error::Error;

use sim_core::catalog::ResourceHarvest;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, MapFile, ResourceNode};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, PlayerSlot, ResourceKind, ResourceNodeId, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn Error>>;

const ME: PlayerId = PlayerId(0);
const SHOWCASE: &str = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");
/// Ticks in a minute: 60 Faster seconds at 25 ticks each.
const TICKS_PER_MINUTE: u64 = 1_500;
/// Warm-up before a window and the window's length, in ticks (plan §6.3).
const WARM_UP: u32 = 750;
const WINDOW: u32 = 3_000;

fn node(kind: ResourceKind, cell: (u32, u32), remaining: u32) -> ResourceNode {
    let centre = |v: u32| SimScalar::from_num(v) + SimScalar::HALF;
    ResourceNode {
        pos: SimVec2::new(centre(cell.0), centre(cell.1)),
        kind,
        remaining,
        cell,
        placed: None,
    }
}

fn flat(nodes: Vec<ResourceNode>) -> Result<GameMap, Box<dyn Error>> {
    Ok(GameMap::from_cells(
        40,
        40,
        vec![MapCell::default_ground(CliffLevel(0)); 1600],
        nodes,
        Vec::new(),
    )?)
}

/// The worker archetype (a harvester), the town hall analogue (a depot with no prerequisite)
/// and the gas extractor.
fn kinds(world: &SimWorld) -> Result<(UnitTypeId, UnitTypeId, UnitTypeId), Box<dyn Error>> {
    let find = |test: &dyn Fn(&sim_core::catalog::UnitArchetype) -> bool, what: &'static str| {
        world
            .catalog
            .archetypes()
            .find(|a| test(a))
            .map(|a| a.unit_type_id)
            .ok_or(what)
    };
    Ok((
        find(&|a| a.capabilities.harvester.is_some(), "a harvester")?,
        find(
            &|a| a.is_resource_dropoff && a.requires_structures.is_empty(),
            "a depot",
        )?,
        find(&|a| a.capabilities.is_gas_extractor, "an extractor")?,
    ))
}

/// The harvest of `kind` in `world`'s catalog.
fn harvest_of(world: &SimWorld, kind: ResourceKind) -> Result<ResourceHarvest, Box<dyn Error>> {
    Ok(*world
        .catalog
        .placement_rules()
        .harvest(kind)
        .ok_or("no harvest row for the kind")?)
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

fn order(world: &mut SimWorld, workers: Vec<EntityId>, order: UnitOrder) {
    step(
        world,
        vec![PlayerCommand {
            player_id: ME,
            entities: workers,
            order,
            queue: false,
        }],
    );
}

/// Steps until `done`, at most `limit` ticks; returns whether it happened.
fn until(world: &mut SimWorld, limit: u32, done: impl Fn(&SimWorld) -> bool) -> bool {
    for _ in 0..limit {
        if done(world) {
            return true;
        }
        step(world, Vec::new());
    }
    done(world)
}

/// A flat world with a hall at (2, 2), a worker at (10, 4) and `nodes`; the bank emptied.
fn flat_world(nodes: Vec<ResourceNode>) -> Result<(SimWorld, EntityId, EntityId), Box<dyn Error>> {
    let mut world = sim_fixtures::try_world(flat(nodes)?, 11)?;
    let (worker_type, hall_type, _) = kinds(&world)?;
    let hall = world.spawn_structure(hall_type, ME, 2, 2)?;
    let worker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(4)),
    )?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    world.player_resources[PlayerSlot::ALL[0]].gas = 0;
    Ok((world, worker, hall))
}

/// Steps a harvesting `worker` until its cargo appears, returning the ticks it gathered.
fn gather_once(world: &mut SimWorld, worker: EntityId) -> Result<u32, Box<dyn Error>> {
    if !until(world, 800, |w| w.entities[&worker].task.gather_ticks > 0) {
        return Err("the worker never started gathering".into());
    }
    let started = world.current_tick;
    if !until(world, 800, |w| w.entities[&worker].cargo.is_some()) {
        return Err("the worker never took a load".into());
    }
    Ok(world.current_tick - started)
}

/// D-1: from the tick its cargo appears, a mineral worker stands where it is for exactly the
/// row's `return_delay_ticks`, counting `return_ticks` down to 0, with no goal; on the next tick
/// it has set off for the depot.
#[test]
fn a_worker_waits_the_return_delay_at_the_field() -> TestResult {
    let (mut world, worker, hall) = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 1_000)])?;
    let mineral = harvest_of(&world, ResourceKind::Mineral)?;
    assert!(
        mineral.return_delay_ticks > 0,
        "the shipped mineral row has a ReturnDelay"
    );
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest {
            target: ResourceNodeId(0),
        },
    );
    gather_once(&mut world, worker)?;
    let at = world.entities[&worker].position;
    assert_eq!(
        world.entities[&worker].task.return_ticks,
        mineral.return_delay_ticks
    );
    for left in (0..mineral.return_delay_ticks).rev() {
        step(&mut world, Vec::new());
        let entity = &world.entities[&worker];
        assert_eq!(entity.position, at, "it waits where it gathered");
        assert_eq!(entity.task.return_ticks, left);
        assert_eq!(entity.task.goal, None);
        assert!(entity.cargo.is_some(), "it holds its load while it waits");
        assert!(entity.is_mineral_walking, "bodies pass a waiting worker");
    }
    step(&mut world, Vec::new());
    let entity = &world.entities[&worker];
    assert_eq!(entity.task.depot, Some(hall));
    assert!(entity.task.goal.is_some(), "it has set off for the depot");
    Ok(())
}

/// D-2: the gas row has no delay, so a gas worker sets off for the depot on the tick after its
/// cargo appears; its gather lasted the gas row's time and its load is the gas row's amount.
#[test]
fn a_gas_worker_leaves_at_once() -> TestResult {
    let (mut world, worker, hall) = flat_world(vec![node(ResourceKind::Gas, (20, 20), 100)])?;
    let gas = harvest_of(&world, ResourceKind::Gas)?;
    assert_eq!(
        gas.return_delay_ticks, 0,
        "the shipped gas row has no ReturnDelay"
    );
    let (_, _, extractor) = kinds(&world)?;
    world.spawn_structure(extractor, ME, 19, 19)?;
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest {
            target: ResourceNodeId(0),
        },
    );
    let gathered = gather_once(&mut world, worker)?;
    assert_eq!(gathered, u32::from(gas.time_ticks));
    let entity = &world.entities[&worker];
    assert_eq!(
        entity.cargo,
        Some(sim_core::Cargo {
            kind: ResourceKind::Gas,
            amount: gas.amount
        })
    );
    assert_eq!(entity.task.return_ticks, 0);
    step(&mut world, Vec::new());
    let entity = &world.entities[&worker];
    assert_eq!(entity.task.depot, Some(hall));
    assert!(entity.task.goal.is_some(), "it has set off for the depot");
    Ok(())
}

/// D-1: the node is drawn down on the tick of the take, not after the wait, and a take that
/// empties a field removes it then; the worker waits where the field stood.
#[test]
fn the_field_is_drawn_down_before_the_wait() -> TestResult {
    let probe = sim_fixtures::try_world(flat(Vec::new())?, 11)?;
    let mineral = harvest_of(&probe, ResourceKind::Mineral)?;
    let (mut world, worker, _) = flat_world(vec![
        node(ResourceKind::Mineral, (12, 4), 1_000),
        node(ResourceKind::Mineral, (12, 8), mineral.amount),
    ])?;
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest {
            target: ResourceNodeId(0),
        },
    );
    gather_once(&mut world, worker)?;
    assert_eq!(world.map.resources()[0].remaining, 1_000 - mineral.amount);
    assert_eq!(
        world.entities[&worker].task.return_ticks,
        mineral.return_delay_ticks
    );
    for _ in 0..mineral.return_delay_ticks {
        step(&mut world, Vec::new());
        assert_eq!(world.map.resources()[0].remaining, 1_000 - mineral.amount);
    }

    // The field holding exactly one load leaves the map on its take, before the wait.
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest {
            target: ResourceNodeId(1),
        },
    );
    if !until(&mut world, 800, |w| w.entities[&worker].cargo.is_none()) {
        return Err("the first load was never delivered".into());
    }
    gather_once(&mut world, worker)?;
    let field = &world.map.resources()[1];
    assert_eq!(field.remaining, 0);
    assert!(
        field.placed.is_none(),
        "the emptied field left the map on the take"
    );
    let entity = &world.entities[&worker];
    assert_eq!(entity.task.return_ticks, mineral.return_delay_ticks);
    let at = entity.position;
    step(&mut world, Vec::new());
    assert_eq!(
        world.entities[&worker].position, at,
        "it waits where the field stood"
    );
    Ok(())
}

/// A flat world whose worker has taken its first load and is one tick into its `ReturnDelay`
/// wait at the field: built the same way every time, so two calls give twin worlds.
fn waiting_world() -> Result<(SimWorld, EntityId), Box<dyn Error>> {
    let (mut world, worker, _) = flat_world(vec![node(ResourceKind::Mineral, (12, 4), 1_000)])?;
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest {
            target: ResourceNodeId(0),
        },
    );
    gather_once(&mut world, worker)?;
    step(&mut world, Vec::new());
    if world.entities[&worker].task.return_ticks == 0 {
        return Err("the worker is not waiting".into());
    }
    Ok((world, worker))
}

/// The tick on which an uninterrupted twin of [`waiting_world`] first leaves the spot where it
/// waits: the first tick of its delivery leg.
fn twin_leaves() -> Result<u32, Box<dyn Error>> {
    let (mut twin, worker) = waiting_world()?;
    let spot = twin.entities[&worker].position;
    for _ in 0..100 {
        step(&mut twin, Vec::new());
        if twin.entities[&worker].position != spot {
            return Ok(twin.current_tick);
        }
    }
    Err("the twin never left".into())
}

/// Steps until `worker` leaves the spot it stands on, at most 100 ticks; returns that tick.
fn leaves(world: &mut SimWorld, worker: EntityId) -> Result<u32, Box<dyn Error>> {
    let spot = world.entities[&worker].position;
    for _ in 0..100 {
        step(world, Vec::new());
        if world.entities[&worker].position != spot {
            return Ok(world.current_tick);
        }
    }
    Err("the worker never left".into())
}

/// Plan `2026-11-23-harvest-occupancy` D-5 (B-160; 5.0.14 `UninterruptibleArray WaitToReturn 1`,
/// core AbilData:379): a `Move` given during the wait does not cut it short. The worker stands
/// until the tick an uninterrupted twin starts its delivery leg and moves from that tick; the
/// cargo stays on it; a later `Harvest` delivers that cargo before it gathers again.
#[test]
fn a_new_order_during_the_wait_starts_after_it() -> TestResult {
    let expected = twin_leaves()?;
    let (mut world, worker) = waiting_world()?;
    let mineral = harvest_of(&world, ResourceKind::Mineral)?;
    let target = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(14));
    order(&mut world, vec![worker], UnitOrder::Move { target });
    assert_eq!(
        world.entities[&worker].order_queue.front(),
        Some(&UnitOrder::Move { target }),
        "the order is taken at once"
    );
    assert_eq!(leaves(&mut world, worker)?, expected);
    let cargo = Some(sim_core::Cargo {
        kind: ResourceKind::Mineral,
        amount: mineral.amount,
    });
    assert_eq!(world.entities[&worker].cargo, cargo);
    for _ in 0..20 {
        step(&mut world, Vec::new());
    }
    assert_eq!(
        world.entities[&worker].cargo, cargo,
        "cargo outlives orders"
    );
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest {
            target: ResourceNodeId(0),
        },
    );
    for _ in 0..800 {
        if world.player_resources[PlayerSlot::ALL[0]].minerals > 0 {
            assert_eq!(
                world.player_resources[PlayerSlot::ALL[0]].minerals,
                mineral.amount
            );
            return Ok(());
        }
        assert_eq!(
            world.entities[&worker].task.gather_ticks, 0,
            "it delivers before it gathers again"
        );
        assert_eq!(world.map.resources()[0].remaining, 1_000 - mineral.amount);
        step(&mut world, Vec::new());
    }
    Err("the cargo was never delivered".into())
}

/// D-5: a queued (Shift) `Move` given during the wait lands behind the `Harvest` order and changes
/// nothing else: the worker follows its uninterrupted twin to the tick.
#[test]
fn a_queued_order_during_the_wait_is_unchanged() -> TestResult {
    let (mut twin, _) = waiting_world()?;
    let (mut world, worker) = waiting_world()?;
    let target = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(14));
    let shift = |world: &mut SimWorld| {
        step(
            world,
            vec![PlayerCommand {
                player_id: ME,
                entities: vec![worker],
                order: UnitOrder::Move { target },
                queue: true,
            }],
        );
    };
    shift(&mut world);
    step(&mut twin, Vec::new());
    assert_eq!(
        world.entities[&worker]
            .order_queue
            .iter()
            .collect::<Vec<_>>(),
        vec![
            &UnitOrder::Harvest {
                target: ResourceNodeId(0)
            },
            &UnitOrder::Move { target }
        ]
    );
    for _ in 0..400 {
        let (ours, theirs) = (&world.entities[&worker], &twin.entities[&worker]);
        assert_eq!(
            (ours.position, ours.cargo, ours.task.return_ticks),
            (theirs.position, theirs.cargo, theirs.task.return_ticks),
            "tick {}",
            world.current_tick
        );
        assert_eq!(
            world.player_resources[PlayerSlot::ALL[0]].minerals,
            twin.player_resources[PlayerSlot::ALL[0]].minerals
        );
        step(&mut world, Vec::new());
        step(&mut twin, Vec::new());
    }
    Ok(())
}

/// D-5: `ReturnCargo` given during the wait is held like any other order: the worker stands
/// until the tick its uninterrupted twin leaves, then walks to the depot and delivers the load.
#[test]
fn return_cargo_during_the_wait_is_held_then_delivers() -> TestResult {
    let expected = twin_leaves()?;
    let (mut world, worker) = waiting_world()?;
    let mineral = harvest_of(&world, ResourceKind::Mineral)?;
    order(&mut world, vec![worker], UnitOrder::ReturnCargo);
    assert_eq!(leaves(&mut world, worker)?, expected);
    if !until(&mut world, 800, |w| {
        w.player_resources[PlayerSlot::ALL[0]].minerals > 0
    }) {
        return Err("the cargo was never delivered".into());
    }
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        mineral.amount
    );
    assert_eq!(world.entities[&worker].cargo, None);
    Ok(())
}

/// Worker 0's trip on a node over the window of §6.3: deliveries, and the ticks between the first
/// and the last delivery split into walking, gathering and waiting.
struct Trips {
    deliveries: u64,
    span: u64,
    walking: u64,
    gathering: u64,
    waiting: u64,
}

impl Trips {
    /// Steps `world` through the warm-up and the window, following `worker`.
    fn measure(world: &mut SimWorld, worker: EntityId) -> Self {
        for _ in 0..WARM_UP {
            step(world, Vec::new());
        }
        let mut held = world.entities[&worker].cargo.is_some();
        let (mut first, mut last) = (None, 0_u64);
        let (mut deliveries, mut walking, mut gathering, mut waiting) = (0, 0, 0, 0);
        let (mut walking_since, mut gathering_since, mut waiting_since) = (0, 0, 0);
        for t in 0..u64::from(WINDOW) {
            step(world, Vec::new());
            let entity = &world.entities[&worker];
            let carrying = entity.cargo.is_some();
            if entity.task.gather_ticks > 0 {
                gathering_since += 1;
            } else if carrying && entity.task.return_ticks > 0 {
                waiting_since += 1;
            } else {
                walking_since += 1;
            }
            if held && !carrying {
                deliveries += 1;
                if first.is_some() {
                    walking += walking_since;
                    gathering += gathering_since;
                    waiting += waiting_since;
                }
                first.get_or_insert(t);
                last = t;
                (walking_since, gathering_since, waiting_since) = (0, 0, 0);
            }
            held = carrying;
        }
        Self {
            deliveries,
            span: last - first.unwrap_or(last),
            walking,
            gathering,
            waiting,
        }
    }

    /// Whole trips between the first and the last delivery.
    const fn trips(&self) -> u64 {
        self.deliveries.saturating_sub(1)
    }

    /// Whether the trip-period rate, `amount` units per mean trip in units per minute, lies in
    /// `[low, high]` hundredths: `low ≤ amount × 1,500 × 100 × trips / span ≤ high`.
    fn rate_within(&self, amount: u32, (low, high): (u64, u64)) -> bool {
        let earned = u64::from(amount) * TICKS_PER_MINUTE * 100 * self.trips();
        low * self.span <= earned && earned <= high * self.span
    }
}

/// A showcase world with slot 0's hall at its start and one worker beside it.
fn showcase_start() -> Result<(SimWorld, EntityId), Box<dyn Error>> {
    let doc = MapFile::from_ron_str(SHOWCASE)?.decode()?;
    let start = doc
        .start_locations
        .iter()
        .find(|s| s.player_slot == 0)
        .ok_or("slot 0")?
        .cell;
    let mut world = sim_fixtures::try_world(doc.to_game_map()?, 7)?;
    let (worker_type, hall_type, _) = kinds(&world)?;
    world.spawn_structure(hall_type, ME, start.0, start.1)?;
    let worker = world.spawn_unit(
        worker_type,
        ME,
        SimVec2::new(
            SimScalar::from_num(start.0 + 6),
            SimScalar::from_num(start.1 + 6),
        ),
    )?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
    world.player_resources[PlayerSlot::ALL[0]].gas = 0;
    Ok((world, worker))
}

/// The showcase node of `kind` on `cell`.
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

/// §6.1: on the showcase's slot-0 start, field (6, 14), the trip period over the window is the
/// walking legs measured in the same run plus the row's `harvest_time_ticks` and
/// `return_delay_ticks`: each whole trip gathers exactly the one and waits exactly the other.
#[test]
fn the_trip_is_the_legs_plus_the_resource_times() -> TestResult {
    let (mut world, worker) = showcase_start()?;
    let mineral = harvest_of(&world, ResourceKind::Mineral)?;
    let field = showcase_node(&world, ResourceKind::Mineral, (6, 14))?;
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest { target: field },
    );
    let trips = Trips::measure(&mut world, worker);
    let whole = trips.trips();
    assert!(whole >= 10, "{whole} whole trips in the window");
    assert_eq!(trips.gathering, u64::from(mineral.time_ticks) * whole);
    assert_eq!(trips.waiting, u64::from(mineral.return_delay_ticks) * whole);
    assert_eq!(
        trips.span,
        trips.walking
            + (u64::from(mineral.time_ticks) + u64::from(mineral.return_delay_ticks)) * whole
    );
    Ok(())
}

/// §6.2 (gas): showcase slot 0's near geyser (10, 1), whose walking legs are within 10 % of the
/// ladder's 63 ticks, earns one worker the game's 61 gas a minute (Liquipedia, Resources) ± 5 %:
/// its trip-period rate lies in [57.95, 64.05].
#[test]
fn a_near_geyser_earns_the_games_rate() -> TestResult {
    let (mut world, worker) = showcase_start()?;
    let gas = harvest_of(&world, ResourceKind::Gas)?;
    let geyser = showcase_node(&world, ResourceKind::Gas, (10, 1))?;
    let (_, _, extractor) = kinds(&world)?;
    world.spawn_structure(extractor, ME, 9, 0)?;
    order(
        &mut world,
        vec![worker],
        UnitOrder::Harvest { target: geyser },
    );
    let trips = Trips::measure(&mut world, worker);
    assert!(trips.trips() >= 10, "{} whole trips", trips.trips());
    assert!(
        trips.rate_within(gas.amount, (5_795, 6_405)),
        "{} gas over {} ticks of {} trips",
        u64::from(gas.amount) * trips.trips(),
        trips.span,
        trips.trips()
    );
    Ok(())
}
