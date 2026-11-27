//! Rally points of production structures (plan `2026-11-23-production-queue` D-3 to D-7; B-60).
//!
//! `SetRally` admission and the new unit's order (D-3, D-4) are checked on a constructed base whose
//! producers and products are read from the shipped catalog by capability. The town hall's
//! auto-rally is checked on the shipped showcase against a node computed here
//! from the map's nodes and the archetype's authored `auto_rally`, and on constructed maps for
//! the cases the showcase does not reach. No identity is named: the hall is whichever archetype
//! carries the capability.
//!
//! Plan `2026-11-25-rallies` adds the routes (WP1: one route per slot, their invariants) and the
//! rally slots as data (WP2, P-2 and P-3): the structure with a worker slot and an army slot is
//! the one whose authored rally ability has a `Workers` slot, and its products are told apart by
//! the harvest capability and the slots' exclusions.

use sim_core::economy::{GameCommandError, GameCommandReceipt};
use sim_core::scenario::load_scenario_world;
use sim_core::{SimWorld, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
use sim_types::{
    AutoRallySpec, EntityId, FollowKind, PlayerCommand, PlayerId, PlayerSlot, RallyRoute,
    RallyTarget, RallyUsers, ResourceNodeId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// The squared raw distance from `point` to the cells `[x, x + w] × [y, y + h]`.
fn squared_gap(point: SimVec2, cell: (u32, u32), size: (u8, u8)) -> i128 {
    let axis = |value: SimScalar, start: u32, extent: u8| {
        let raw = i128::from(value.raw());
        let low = i128::from(start) << 32;
        let high = (i128::from(start) + i128::from(extent)) << 32;
        (low - raw).max(raw - high).max(0)
    };
    let (dx, dy) = (axis(point.x, cell.0, size.0), axis(point.y, cell.1, size.1));
    dx * dx + dy * dy
}

fn squared(a: SimVec2, b: SimVec2) -> i128 {
    let dx = i128::from(a.x.raw()) - i128::from(b.x.raw());
    let dy = i128::from(a.y.raw()) - i128::from(b.y.raw());
    dx * dx + dy * dy
}

/// The node D-7 names for a hall at `centre`: standing, not empty, of an accepted kind, its
/// cells within the radius; the farthest centre, the lower id on a tie.
fn expected_node(world: &SimWorld, centre: SimVec2, spec: AutoRallySpec) -> Option<ResourceNodeId> {
    let reach = i128::from(spec.radius.raw()).pow(2);
    let mut candidates: Vec<(i128, u32)> = (0_u32..)
        .zip(world.map.resources())
        .filter_map(|(id, node)| {
            let placed = node.placed?.footprint;
            let near = squared_gap(centre, placed.top_left, (placed.width, placed.height)) <= reach;
            (near && node.remaining > 0 && spec.kinds.contains(node.kind))
                .then(|| (squared(centre, node.pos), id))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    candidates.first().map(|&(_, id)| ResourceNodeId(id))
}

/// The town halls of `world`: each entity whose archetype carries an auto-rally, with its spec.
fn halls(world: &SimWorld) -> Vec<(EntityId, AutoRallySpec)> {
    world
        .entities
        .values()
        .filter_map(|entity| {
            let spec = world
                .catalog
                .get(entity.unit_type_id)
                .ok()?
                .capabilities
                .auto_rally?;
            Some((entity.id, spec))
        })
        .collect()
}

fn node(x: u32, y: u32, kind: ResourceKind) -> ResourceNode {
    ResourceNode {
        pos: SimVec2::new(
            SimScalar::from_num(x) + SimScalar::HALF,
            SimScalar::from_num(y) + SimScalar::HALF,
        ),
        kind,
        remaining: 1500,
        cell: (x, y),
        placed: None,
    }
}

/// A 32 × 32 ground world with `nodes` and the shipped catalog, and a hall type.
fn constructed(
    nodes: Vec<ResourceNode>,
) -> Result<(SimWorld, UnitTypeId), Box<dyn std::error::Error>> {
    let catalogs =
        sim_core::catalog::RuntimeCatalogs::load_from_directory(assets().join("catalog"))?;
    let hall = catalogs
        .units
        .archetypes()
        .find(|arch| arch.capabilities.auto_rally.is_some())
        .ok_or("the shipped catalog has no auto-rallying structure")?
        .unit_type_id;
    let map = GameMap::from_cells(
        32,
        32,
        vec![MapCell::default_ground(CliffLevel(0)); 32 * 32],
        nodes,
        vec![],
    )?;
    let world = SimWorld::with_catalogs(map, 7, catalogs.units, UpgradeCatalog::default())?;
    Ok((world, hall))
}

/// WP1 (D-7, S6): every showcase town hall rallies to the farthest mineral field whose cells
/// come within the authored radius; the expected node is computed from the map.
#[test]
fn a_town_hall_rallies_to_the_farthest_mineral_field_in_reach() -> TestResult {
    let world = load_scenario_world(&assets(), Path::new("scenarios/showcase.ron"))?;
    let halls = halls(&world);
    assert_eq!(halls.len(), 2, "one hall per showcase start");
    for (hall, spec) in halls {
        let centre = world.entities[&hall].position;
        let expected = expected_node(&world, centre, spec).ok_or("no field in reach")?;
        let chosen = world.map.resources()[expected.0 as usize].kind;
        assert_eq!(chosen, ResourceKind::Mineral);
        assert_eq!(
            world.entities[&hall].rallies,
            [RallyRoute::single(0, RallyTarget::Node(expected))],
            "hall {hall:?}"
        );
    }
    Ok(())
}

/// WP1 (D-7): a hall with no field within its radius has no rally, and a geyser is never chosen,
/// even when it is in reach and farther than the field.
#[test]
fn a_hall_without_a_field_in_reach_has_no_rally_and_a_geyser_is_never_chosen() -> TestResult {
    // A hall of the shipped footprint at (2, 2); a field far away.
    let (mut world, hall_type) = constructed(vec![node(26, 26, ResourceKind::Mineral)])?;
    let hall = world.spawn_structure(hall_type, PlayerId(0), 2, 2)?;
    assert_eq!(world.entities[&hall].rallies, []);
    // A field and a farther geyser, both within reach of a hall at (12, 12).
    let (mut world, hall_type) = constructed(vec![
        node(14, 19, ResourceKind::Mineral),
        node(20, 19, ResourceKind::Gas),
    ])?;
    let hall = world.spawn_structure(hall_type, PlayerId(0), 12, 12)?;
    let centre = world.entities[&hall].position;
    let geyser = world.map.resources()[1].clone();
    let field = world.map.resources()[0].clone();
    let spec = world
        .catalog
        .get(hall_type)?
        .capabilities
        .auto_rally
        .ok_or("hall without auto-rally")?;
    let reach = i128::from(spec.radius.raw()).pow(2);
    let geyser_cells = geyser.placed.ok_or("geyser not placed")?.footprint;
    assert!(
        squared_gap(
            centre,
            geyser_cells.top_left,
            (geyser_cells.width, geyser_cells.height)
        ) <= reach,
        "the geyser is in reach"
    );
    assert!(squared(centre, geyser.pos) > squared(centre, field.pos));
    assert_eq!(
        world.entities[&hall].rallies,
        [RallyRoute::single(0, RallyTarget::Node(ResourceNodeId(0)))]
    );
    // A geyser alone gives no rally.
    let (mut world, hall_type) = constructed(vec![node(20, 19, ResourceKind::Gas)])?;
    let hall = world.spawn_structure(hall_type, PlayerId(0), 12, 12)?;
    assert_eq!(world.entities[&hall].rallies, []);
    Ok(())
}

/// A 32 × 32 ground world with a mineral field and a geyser, a town hall and a soldier producer
/// of the shipped catalog, and a bank that pays for anything; the products read from data.
struct Base {
    world: SimWorld,
    /// A completed structure with an auto-rally that trains a harvester.
    hall: EntityId,
    /// A completed structure that trains a ground soldier without the harvest capability.
    barracks: EntityId,
    worker: UnitTypeId,
    soldier: UnitTypeId,
}

/// The first ground product `arch` trains that harvests or not as `harvests` says.
fn first_product(
    world: &SimWorld,
    arch: &sim_core::UnitArchetype,
    harvests: bool,
) -> Option<UnitTypeId> {
    arch.trains_units.iter().find_map(|slug| {
        world
            .catalog
            .get_by_slug(slug)
            .filter(|p| {
                !matches!(p.shape, ShapeKind::BuildingDome { .. })
                    && p.mover_type == sim_types::MoverType::Ground
                    && p.capabilities.harvester.is_some() == harvests
            })
            .map(|p| p.unit_type_id)
    })
}

/// Where the field (in the hall's auto-rally reach) and the geyser (out of it) are anchored.
const FIELD: (u32, u32) = (12, 6);
const GEYSER: (u32, u32) = (24, 24);

impl Base {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (mut world, hall_type) = constructed(vec![
            node(FIELD.0, FIELD.1, ResourceKind::Mineral),
            node(GEYSER.0, GEYSER.1, ResourceKind::Gas),
        ])?;
        let hall_arch = world.catalog.get(hall_type)?;
        let worker =
            first_product(&world, hall_arch, true).ok_or("the hall trains no harvester")?;
        let faction = hall_arch.faction;
        let (barracks_type, soldier) = world
            .catalog
            .archetypes()
            .filter(|arch| arch.faction == faction)
            .find_map(|arch| first_product(&world, arch, false).map(|p| (arch.unit_type_id, p)))
            .ok_or("no producer of a ground soldier")?;
        let hall = world.spawn_structure(hall_type, PlayerId(0), 4, 4)?;
        let barracks = world.spawn_structure(barracks_type, PlayerId(0), 4, 20)?;
        let bank = &mut world.player_resources[PlayerSlot::ALL[0]];
        bank.minerals = 100_000;
        bank.gas = 100_000;
        Ok(Self {
            world,
            hall,
            barracks,
            worker,
            soldier,
        })
    }

    /// One tick with `commands`, checking that nothing is diagnosed.
    fn tick_with(&mut self, commands: Vec<PlayerCommand>) {
        self.world.step(&TickBundle {
            tick: Tick(self.world.current_tick + 1),
            commands,
            state_checksum: None,
        });
        assert_eq!(
            self.world.diagnostics(),
            [],
            "tick {}",
            self.world.current_tick
        );
    }

    fn tick(&mut self) {
        self.tick_with(Vec::new());
    }

    /// Issues `order` from player 0 to `entities` and returns the economy's verdict.
    fn command(
        &mut self,
        entities: Vec<EntityId>,
        order: UnitOrder,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.tick_with(vec![PlayerCommand {
            player_id: PlayerId(0),
            entities,
            order,
            queue: false,
        }]);
        self.world
            .game_command_outcomes()
            .first()
            .map_or(Err(GameCommandError::UnsupportedOrder), |outcome| {
                outcome.result.clone()
            })
    }

    /// Issues `SetRally` from player 0 to `producer`, Shift-queued when `queue`.
    fn shift_rally(
        &mut self,
        producer: EntityId,
        target: RallyTarget,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.tick_with(vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![producer],
            order: UnitOrder::SetRally { target, slot: None },
            queue,
        }]);
        self.world
            .game_command_outcomes()
            .first()
            .map_or(Err(GameCommandError::UnsupportedOrder), |outcome| {
                outcome.result.clone()
            })
    }

    /// The waypoints of `producer`'s slot-0 route.
    fn waypoints(&self, producer: EntityId) -> Vec<RallyTarget> {
        self.world.entities[&producer]
            .rallies
            .first()
            .map_or_else(Vec::new, |route| route.waypoints.clone())
    }

    /// The child's whole order queue.
    fn orders(&self, id: EntityId) -> Vec<UnitOrder> {
        self.world.entities[&id]
            .order_queue
            .iter()
            .cloned()
            .collect()
    }

    fn set_rally(&mut self, producer: EntityId, target: RallyTarget) -> TestResult {
        self.command(vec![producer], UnitOrder::SetRally { target, slot: None })?;
        assert_eq!(
            self.world.entities[&producer].rallies,
            [RallyRoute::single(0, target)]
        );
        Ok(())
    }

    /// The first waypoint of `producer`'s slot-0 route: the one rally of a one-slot producer.
    fn rally(&self, producer: EntityId) -> Option<RallyTarget> {
        let route = self.world.entities[&producer].rallies.first()?;
        assert_eq!(route.slot, 0);
        route.waypoints.first().copied()
    }

    /// Trains one `product` at `producer` and steps until it appears; the child and the
    /// positions every entity had before the tick it appeared in.
    fn train(
        &mut self,
        producer: EntityId,
        product: UnitTypeId,
    ) -> Result<(EntityId, BTreeMap<EntityId, SimVec2>), Box<dyn std::error::Error>> {
        self.world
            .try_queue_production(PlayerId(0), &[producer], product)?;
        self.finish(product)
    }

    /// Steps until a `product` appears; the child and the positions every entity had before the
    /// tick it appeared in.
    fn finish(
        &mut self,
        product: UnitTypeId,
    ) -> Result<(EntityId, BTreeMap<EntityId, SimVec2>), Box<dyn std::error::Error>> {
        let limit = self.world.catalog.get(product)?.build_time_ticks + 2;
        for _ in 0..limit {
            let before: BTreeMap<EntityId, SimVec2> = self
                .world
                .entities
                .values()
                .map(|entity| (entity.id, entity.position))
                .collect();
            let next = self.world.next_entity_id;
            self.tick();
            if self.world.next_entity_id > next {
                let child = EntityId(next);
                assert_eq!(self.world.entities[&child].unit_type_id, product);
                return Ok((child, before));
            }
        }
        Err("the product never appeared".into())
    }

    fn front_order(&self, id: EntityId) -> Option<UnitOrder> {
        self.world.entities[&id].order_queue.front().cloned()
    }

    /// The child's order is a `Move` to the node: movement admission moves a target inside the
    /// node's footprint to the nearest point the child can stand on (S-4), so the target lies
    /// within the footprint's half-diagonal and the child's terrain radius of the node's centre.
    fn assert_walks_to_node(&self, child: EntityId, index: u32) -> TestResult {
        let node = &self.world.map.resources()[index as usize];
        let placed = node.placed.ok_or("the node stands")?.footprint;
        let Some(UnitOrder::Move { target }) = self.front_order(child) else {
            return Err(format!("{:?}", self.front_order(child)).into());
        };
        let reach =
            f64::from(placed.width.max(placed.height)) / 2.0 * std::f64::consts::SQRT_2 + 1.0;
        let gap = (target - node.pos).length_sq().to_f32();
        assert!(
            f64::from(gap).sqrt() <= reach,
            "{target:?} against {:?}",
            node.pos
        );
        Ok(())
    }
}

/// D-3: `SetRally` sets the rally of every structure that trains units, and nothing else; a
/// selection with none is refused, as are a point off the map, a unit that is dead or absent,
/// and a node not on the map. Refusals change nothing. Another player's unit is a valid target
/// since plan `2026-11-25-rallies` WP4 (D-6), so it is no longer among the refusals.
#[test]
fn set_rally_is_admitted_only_for_producers() -> TestResult {
    let mut b = Base::new()?;
    let point = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    let drone = b.world.spawn_unit(b.worker, PlayerId(0), point(12, 12))?;
    let here = RallyTarget::Point(point(16, 16));
    assert_eq!(
        b.command(
            vec![b.barracks, drone],
            UnitOrder::SetRally {
                target: here,
                slot: None
            }
        ),
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );
    assert_eq!(b.rally(b.barracks), Some(here));
    assert_eq!(b.rally(drone), None);
    assert_eq!(
        b.command(
            vec![drone],
            UnitOrder::SetRally {
                target: here,
                slot: None
            }
        ),
        Err(GameCommandError::NoCapableProducer)
    );
    let dead = b.world.spawn_unit(b.worker, PlayerId(0), point(12, 16))?;
    b.world.entities.get_mut(&dead).ok_or("dead absent")?.hp = SimScalar::ZERO;
    let off = SimVec2::new(SimScalar::from_i32(33), SimScalar::from_i32(4));
    for (target, refusal) in [
        (
            RallyTarget::Point(off),
            GameCommandError::RallyOutsideMap(off),
        ),
        (
            RallyTarget::Unit(dead),
            GameCommandError::InvalidRallyTarget(dead),
        ),
        (
            RallyTarget::Unit(EntityId(9_999)),
            GameCommandError::InvalidRallyTarget(EntityId(9_999)),
        ),
        (
            RallyTarget::Node(ResourceNodeId(7)),
            GameCommandError::UnknownResourceNode(ResourceNodeId(7)),
        ),
    ] {
        assert_eq!(
            b.command(vec![b.barracks], UnitOrder::SetRally { target, slot: None }),
            Err(refusal)
        );
        assert_eq!(b.rally(b.barracks), Some(here));
    }
    Ok(())
}

/// D-3: a rally on the producer itself clears its rally, as targeting the building does in the
/// game.
#[test]
fn a_rally_on_the_producer_itself_clears_it() -> TestResult {
    let mut b = Base::new()?;
    assert!(b.rally(b.hall).is_some(), "the hall auto-rallied");
    b.command(
        vec![b.hall],
        UnitOrder::SetRally {
            target: RallyTarget::Unit(b.hall),
            slot: None,
        },
    )?;
    assert_eq!(b.rally(b.hall), None);
    Ok(())
}

/// D-4 (S7): a unit made by a structure rallied to a point is ordered there on the tick it
/// appears, and arrives.
#[test]
fn a_unit_moves_to_a_point_rally() -> TestResult {
    let mut b = Base::new()?;
    let target = SimVec2::new(SimScalar::from_num(14.5), SimScalar::from_num(26.5));
    b.set_rally(b.barracks, RallyTarget::Point(target))?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert_eq!(b.front_order(child), Some(UnitOrder::Move { target }));
    for _ in 0..400 {
        if b.world.entities[&child].order_queue.is_empty() {
            break;
        }
        b.tick();
    }
    let at = b.world.entities[&child].position;
    let gap = (at - target).length_sq();
    assert!(
        gap < SimScalar::ONE,
        "the child stands at its rally: {at:?}"
    );
    Ok(())
}

/// D-4 (S7): "Setting the Rally Point … on mineral patches … will automatically command new
/// workers to begin gathering those resources."
#[test]
fn a_worker_rallied_to_a_mineral_field_harvests_it() -> TestResult {
    let mut b = Base::new()?;
    let field = ResourceNodeId(0);
    b.set_rally(b.hall, RallyTarget::Node(field))?;
    let (child, _) = b.train(b.hall, b.worker)?;
    assert_eq!(
        b.front_order(child),
        Some(UnitOrder::Harvest { target: field })
    );
    Ok(())
}

/// D-4: a unit that cannot harvest, rallied to a field, walks to the field's centre.
#[test]
fn a_soldier_rallied_to_a_field_walks_to_it() -> TestResult {
    let mut b = Base::new()?;
    let field = ResourceNodeId(0);
    b.set_rally(b.barracks, RallyTarget::Node(field))?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    b.assert_walks_to_node(child, 0)?;
    Ok(())
}

/// D-4: a geyser yields only through the player's extractor, so a worker rallied to a bare
/// geyser is refused the harvest and walks to the geyser's centre instead.
#[test]
fn a_worker_rallied_to_a_geyser_without_an_extractor_walks_to_it() -> TestResult {
    let mut b = Base::new()?;
    let geyser = ResourceNodeId(1);
    b.set_rally(b.hall, RallyTarget::Node(geyser))?;
    let (child, _) = b.train(b.hall, b.worker)?;
    b.assert_walks_to_node(child, 1)?;
    Ok(())
}

/// D-4, B-163 (fixed by plan `2026-11-26-follow-order` D-9, P-9): a unit rallied to a walking
/// friend follows it, as 5.0.14's does, instead of walking to where the friend stood when it left
/// the structure.
#[test]
fn a_unit_rallied_to_a_friend_follows_it() -> TestResult {
    let mut b = Base::new()?;
    let start = SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(16.5));
    let friend = b.world.spawn_unit(b.soldier, PlayerId(0), start)?;
    b.set_rally(b.barracks, RallyTarget::Unit(friend))?;
    let away = SimVec2::new(SimScalar::from_num(28.5), SimScalar::from_num(28.5));
    b.world
        .try_queue_production(PlayerId(0), &[b.barracks], b.soldier)?;
    // The friend sets off a few ticks before the soldier leaves, so it is walking then.
    while b.world.entities[&b.barracks].production_queue[0].remaining_ticks > 4 {
        b.tick();
    }
    b.tick_with(vec![PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![friend],
        order: UnitOrder::Move { target: away },
        queue: false,
    }]);
    let (child, before) = b.finish(b.soldier)?;
    let stood = before[&friend];
    assert_ne!(stood, start, "the friend was walking");
    let follow = UnitOrder::Follow {
        target: friend,
        kind: FollowKind::Move,
    };
    assert_eq!(b.front_order(child), Some(follow.clone()));
    b.tick();
    assert_ne!(
        b.world.entities[&friend].position, stood,
        "the friend walks on"
    );
    assert_eq!(b.front_order(child), Some(follow));
    Ok(())
}

/// D-4 (S7): "If the rally target dies before the unit exits the production structure … the
/// new unit will not go anywhere … as if there'd never been a rally point": no order, and the
/// rally is cleared.
#[test]
fn a_dead_rally_target_sends_no_order_and_clears_the_rally() -> TestResult {
    let mut b = Base::new()?;
    let at = SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(16.5));
    let friend = b.world.spawn_unit(b.soldier, PlayerId(0), at)?;
    b.set_rally(b.barracks, RallyTarget::Unit(friend))?;
    b.world.entities.get_mut(&friend).ok_or("friend absent")?.hp = SimScalar::ZERO;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert!(!b.world.entities.contains_key(&friend), "reaped");
    assert_eq!(b.front_order(child), None);
    assert_eq!(b.rally(b.barracks), None);
    Ok(())
}

/// D-4 (S7): "Rally Points set on transports … will load the newly created units into them."
#[test]
fn a_unit_rallied_to_its_carrier_boards_it() -> TestResult {
    let mut b = Base::new()?;
    let carrier_type = b
        .world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.transport_capacity > 0)
        .ok_or("no carrier in the catalog")?
        .unit_type_id;
    let at = SimVec2::new(SimScalar::from_num(12.5), SimScalar::from_num(22.5));
    let carrier = b.world.spawn_unit(carrier_type, PlayerId(0), at)?;
    b.set_rally(b.barracks, RallyTarget::Unit(carrier))?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert_eq!(b.front_order(child), Some(UnitOrder::Load { carrier }));
    for _ in 0..400 {
        if b.world.entities[&child].is_loaded_in_transport() {
            return Ok(());
        }
        b.tick();
    }
    Err("the child never boarded".into())
}

/// D-7 acting (S6, S7): the town hall's auto-rally sends the workers it trains to harvest.
#[test]
fn a_town_halls_auto_rally_sends_its_workers_to_harvest() -> TestResult {
    let mut b = Base::new()?;
    let Some(RallyTarget::Node(field)) = b.rally(b.hall) else {
        return Err("the hall has no node rally".into());
    };
    let (child, _) = b.train(b.hall, b.worker)?;
    assert_eq!(
        b.front_order(child),
        Some(UnitOrder::Harvest { target: field })
    );
    Ok(())
}

/// P-4: without a rally a new unit gets no order and stands at its exit, as before rallies.
#[test]
fn a_world_without_a_rally_is_unchanged() -> TestResult {
    let mut b = Base::new()?;
    b.command(
        vec![b.hall],
        UnitOrder::SetRally {
            target: RallyTarget::Unit(b.hall),
            slot: None,
        },
    )?;
    for (producer, product) in [(b.hall, b.worker), (b.barracks, b.soldier)] {
        assert_eq!(b.rally(producer), None);
        let (child, _) = b.train(producer, product)?;
        assert_eq!(b.front_order(child), None);
    }
    Ok(())
}

/// Plan `2026-11-25-rallies` WP1 (D-2, D-3): a rally is a route of one waypoint in slot 0,
/// whether the command names no slot (the right click) or slot 0; slot 1 does not exist on a
/// one-slot producer, so no recipient takes it and the routes are unchanged.
#[test]
fn a_rally_is_a_route_of_one_waypoint_in_slot_zero() -> TestResult {
    let mut b = Base::new()?;
    let point = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    for (slot, target) in [
        (None, RallyTarget::Point(point(16, 16))),
        (Some(0), RallyTarget::Point(point(18, 12))),
        (Some(0), RallyTarget::Node(ResourceNodeId(0))),
    ] {
        assert_eq!(
            b.command(vec![b.barracks], UnitOrder::SetRally { target, slot }),
            Ok(GameCommandReceipt::Accepted { recipients: 1 })
        );
        assert_eq!(
            b.world.entities[&b.barracks].rallies,
            [RallyRoute {
                slot: 0,
                waypoints: vec![target]
            }]
        );
    }
    let before = b.world.entities[&b.barracks].rallies.clone();
    assert_eq!(
        b.command(
            vec![b.barracks],
            UnitOrder::SetRally {
                target: RallyTarget::Point(point(4, 28)),
                slot: Some(1),
            }
        ),
        Err(GameCommandError::NoRallySlotTakes)
    );
    assert_eq!(b.world.entities[&b.barracks].rallies, before);
    Ok(())
}

/// Plan `2026-11-25-rallies` WP1 (D-2): routes that are unsorted, empty or of a slot beyond the
/// rally ability are an invariant failure. `SetRally` refuses with the invariant and changes
/// nothing, and a unit made there gets no order while the failure is reported as a diagnostic.
#[test]
fn corrupt_routes_are_invariant_failures() -> TestResult {
    use sim_core::production::ProductionInvariant;
    use sim_core::SimulationDiagnostic;
    let point = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    let target = RallyTarget::Point(point(16, 16));
    let corrupt = [
        vec![RallyRoute::single(1, target), RallyRoute::single(0, target)],
        vec![RallyRoute::single(0, target), RallyRoute::single(0, target)],
        vec![RallyRoute {
            slot: 0,
            waypoints: Vec::new(),
        }],
        vec![RallyRoute::single(9, target)],
    ];
    for routes in corrupt {
        let mut b = Base::new()?;
        b.world
            .entities
            .get_mut(&b.barracks)
            .ok_or("barracks absent")?
            .rallies
            .clone_from(&routes);
        b.world.step(&TickBundle {
            tick: Tick(b.world.current_tick + 1),
            commands: vec![PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![b.barracks],
                order: UnitOrder::SetRally { target, slot: None },
                queue: false,
            }],
            state_checksum: None,
        });
        let outcome = b.world.game_command_outcomes()[0].result.clone();
        assert!(
            matches!(
                outcome,
                Err(GameCommandError::Invariant(
                    ProductionInvariant::EntityState { entity, .. }
                )) if entity == b.barracks
            ),
            "{routes:?}: {outcome:?}"
        );
        assert_eq!(b.world.entities[&b.barracks].rallies, routes);
        b.world
            .try_queue_production(PlayerId(0), &[b.barracks], b.soldier)?;
        let limit = b.world.catalog.get(b.soldier)?.build_time_ticks + 2;
        let next = b.world.next_entity_id;
        let mut reported = false;
        for _ in 0..limit {
            b.world.step(&TickBundle {
                tick: Tick(b.world.current_tick + 1),
                commands: Vec::new(),
                state_checksum: None,
            });
            reported |= b.world.diagnostics().iter().any(|diagnostic| {
                matches!(
                    diagnostic,
                    SimulationDiagnostic::ProductionInvariant {
                        source: ProductionInvariant::EntityState { entity, .. },
                        ..
                    } if *entity == b.barracks
                )
            });
            if b.world.next_entity_id > next {
                break;
            }
        }
        let child = EntityId(next);
        assert!(
            b.world.entities.contains_key(&child),
            "{routes:?}: no child"
        );
        assert!(reported, "{routes:?}: the spawn reported nothing");
        assert_eq!(b.front_order(child), None, "{routes:?}");
    }
    Ok(())
}

/// The rally slots of a Hatchery analogue: the catalog's structure whose rally ability has a slot
/// for workers and one for non-workers (5.0.14 `RallyHatchery`), read by capability, with its
/// products sorted by which slot uses them.
struct TwoSlotBase {
    base: Base,
    hatch: EntityId,
    /// The slot used by non-workers, which refuses resources.
    army_slot: u8,
    /// The slot used by workers, which a right click on the ground does not set.
    worker_slot: u8,
    /// A worker it trains (the harvest capability).
    worker: UnitTypeId,
    /// A non-worker it trains that uses the army slot.
    army: UnitTypeId,
    /// A product every slot excludes (the Queen analogue).
    excluded: UnitTypeId,
}

impl TwoSlotBase {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut base = Base::new()?;
        let arch = base
            .world
            .catalog
            .archetypes()
            .find(|arch| {
                arch.rally.as_ref().is_some_and(|rally| {
                    rally.slots.len() > 1
                        && rally
                            .slots
                            .iter()
                            .any(|slot| slot.used_by == RallyUsers::Workers)
                })
            })
            .ok_or("no structure with a worker rally slot")?
            .clone();
        let rally = arch.rally.clone().ok_or("no rally")?;
        let slot_of = |users: RallyUsers| -> Result<u8, Box<dyn std::error::Error>> {
            let index = rally
                .slots
                .iter()
                .position(|slot| slot.used_by == users)
                .ok_or("no such slot")?;
            Ok(u8::try_from(index)?)
        };
        let (army_slot, worker_slot) = (
            slot_of(RallyUsers::NonWorkers)?,
            slot_of(RallyUsers::Workers)?,
        );
        let excluded_by_all = |slug: &str| {
            rally
                .slots
                .iter()
                .all(|slot| slot.excludes.iter().any(|s| s == slug))
        };
        let mut products = arch
            .trains_units
            .iter()
            .filter_map(|slug| base.world.catalog.get_by_slug(slug))
            .filter(|p| !matches!(p.shape, ShapeKind::BuildingDome { .. }));
        let worker = products
            .clone()
            .find(|p| p.capabilities.harvester.is_some())
            .ok_or("no worker")?
            .unit_type_id;
        let army = products
            .clone()
            .find(|p| p.capabilities.harvester.is_none() && !excluded_by_all(&p.slug))
            .ok_or("no army product")?
            .unit_type_id;
        let excluded = products
            .find(|p| excluded_by_all(&p.slug))
            .ok_or("no excluded product")?
            .unit_type_id;
        // The prerequisites of every product, so each can be trained; the structure itself
        // stands with the field (anchored at `FIELD`) in its auto-rally's reach.
        let mut cell = 2;
        for product in [worker, army, excluded] {
            let required = base.world.catalog.get(product)?.requires_structures.clone();
            for slug in required {
                let id = base.world.catalog.id_by_slug(&slug).ok_or("prerequisite")?;
                base.world.spawn_structure(id, PlayerId(0), 24, cell)?;
                cell += 6;
            }
        }
        let hatch = base
            .world
            .spawn_structure(arch.unit_type_id, PlayerId(0), 14, 9)?;
        Ok(Self {
            base,
            hatch,
            army_slot,
            worker_slot,
            worker,
            army,
            excluded,
        })
    }

    fn routes(&self) -> &[RallyRoute] {
        &self.base.world.entities[&self.hatch].rallies
    }

    fn set(
        &mut self,
        target: RallyTarget,
        slot: Option<u8>,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        let hatch = self.hatch;
        self.base
            .command(vec![hatch], UnitOrder::SetRally { target, slot })
    }
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// P-2 (S3, S7, S8): the Hatchery analogue's auto-rally fills its worker slot only, the slot the
/// data names.
#[test]
fn the_hatchery_auto_rally_fills_the_worker_slot() -> TestResult {
    let h = TwoSlotBase::new()?;
    let spec = h
        .base
        .world
        .catalog
        .get(h.base.world.entities[&h.hatch].unit_type_id)?
        .capabilities
        .auto_rally
        .ok_or("no auto-rally")?;
    assert_eq!(spec.slot, h.worker_slot);
    assert_eq!(
        h.routes(),
        [RallyRoute::single(
            h.worker_slot,
            RallyTarget::Node(ResourceNodeId(0))
        )]
    );
    Ok(())
}

/// P-2 (S4, S8): a right click on a resource sets the worker rally, since the army slot refuses
/// resources.
#[test]
fn a_right_click_on_a_field_sets_the_worker_rally() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let geyser = RallyTarget::Node(ResourceNodeId(1));
    assert_eq!(
        h.set(geyser, None),
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );
    assert_eq!(h.routes(), [RallyRoute::single(h.worker_slot, geyser)]);
    Ok(())
}

/// P-2 (S3 `SetOnGround 0`, S8): a right click on the ground, or on a unit, sets the army rally
/// and leaves the worker rally as it was.
#[test]
fn a_right_click_on_the_ground_sets_the_army_rally() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let worker_route = RallyRoute::single(h.worker_slot, RallyTarget::Node(ResourceNodeId(0)));
    let ground = RallyTarget::Point(at(16.5, 26.5));
    h.set(ground, None)?;
    let mut expected = vec![
        RallyRoute::single(h.army_slot, ground),
        worker_route.clone(),
    ];
    expected.sort_by_key(|route| route.slot);
    assert_eq!(h.routes(), expected);
    let friend = h
        .base
        .world
        .spawn_unit(h.base.soldier, PlayerId(0), at(16.5, 16.5))?;
    h.set(RallyTarget::Unit(friend), None)?;
    let mut expected = vec![
        RallyRoute::single(h.army_slot, RallyTarget::Unit(friend)),
        worker_route,
    ];
    expected.sort_by_key(|route| route.slot);
    assert_eq!(h.routes(), expected);
    Ok(())
}

/// P-2 (S8: "press the G key and left-click"): the card's worker entry sets the worker rally on
/// the ground, which a right click cannot.
#[test]
fn the_worker_entry_sets_the_worker_rally_on_the_ground() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let ground = RallyTarget::Point(at(16.5, 26.5));
    h.set(ground, Some(h.worker_slot))?;
    assert_eq!(h.routes(), [RallyRoute::single(h.worker_slot, ground)]);
    Ok(())
}

/// P-2 (S4): the army entry refuses a resource, and nothing changes.
#[test]
fn the_army_rally_refuses_a_resource() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let before = h.routes().to_vec();
    assert_eq!(
        h.set(RallyTarget::Node(ResourceNodeId(1)), Some(h.army_slot)),
        Err(GameCommandError::NoRallySlotTakes)
    );
    assert_eq!(h.routes(), before);
    Ok(())
}

/// P-2 (S3, S8): a worker uses the worker rally and a non-worker the army rally.
#[test]
fn workers_and_non_workers_use_their_own_slots() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let target = at(16.5, 26.5);
    h.set(RallyTarget::Point(target), None)?;
    let (worker, _) = h.base.train(h.hatch, h.worker)?;
    assert_eq!(
        h.base.front_order(worker),
        Some(UnitOrder::Harvest {
            target: ResourceNodeId(0)
        })
    );
    let (army, _) = h.base.train(h.hatch, h.army)?;
    assert_eq!(h.base.front_order(army), Some(UnitOrder::Move { target }));
    Ok(())
}

/// P-2, B-217 (S5 `NotQueen`, S8: "the queen is independently created"): the product every slot
/// excludes uses no rally, whatever the routes hold; and a non-worker gets no order while only
/// the worker rally is set (no fall-through).
#[test]
fn the_queen_analogue_uses_no_rally() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let (army, _) = h.base.train(h.hatch, h.army)?;
    assert_eq!(
        h.base.front_order(army),
        None,
        "only the worker rally is set"
    );
    h.set(RallyTarget::Point(at(16.5, 26.5)), None)?;
    let (excluded, _) = h.base.train(h.hatch, h.excluded)?;
    assert_eq!(h.base.front_order(excluded), None);
    Ok(())
}

/// P-2 (D-4): the exit is chosen nearest the first waypoint of the slot the product uses: a
/// worker leaves on the side of its field, a non-worker on the side of the army rally.
#[test]
fn the_exit_faces_the_products_slot() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let field = h.base.world.map.resources()[0].pos;
    let south = at(6.5, 30.5);
    h.set(RallyTarget::Point(south), None)?;
    let gap = |a: SimVec2, b: SimVec2| (a - b).length_sq();
    let (worker, _) = h.base.train(h.hatch, h.worker)?;
    let (army, _) = h.base.train(h.hatch, h.army)?;
    let (worker_at, army_at) = (
        h.base.world.entities[&worker].position,
        h.base.world.entities[&army].position,
    );
    assert!(
        gap(worker_at, field) < gap(army_at, field),
        "{worker_at:?} {army_at:?}"
    );
    assert!(
        gap(army_at, south) < gap(worker_at, south),
        "{worker_at:?} {army_at:?}"
    );
    Ok(())
}

/// P-3: a producer with one slot and a single-waypoint rally gives the same order and the same
/// exit as the default rally ability of plan `2026-11-23-production-queue` (the archetype
/// without a spec), for a point, a field and a friend.
#[test]
fn a_one_slot_producer_is_unchanged() -> TestResult {
    let shipped = Base::new()?;
    let barracks_type = shipped.world.entities[&shipped.barracks].unit_type_id;
    let spec = shipped.world.catalog.get(barracks_type)?.rally.clone();
    assert!(
        spec.as_ref().is_some_and(|spec| spec.slots.len() == 1),
        "{spec:?}"
    );
    let friend_at = at(16.5, 16.5);
    for target in [
        RallyTarget::Point(at(14.5, 26.5)),
        RallyTarget::Node(ResourceNodeId(0)),
        RallyTarget::Unit(EntityId(0)),
    ] {
        let mut outcomes = Vec::new();
        for authored in [true, false] {
            let mut b = Base::new()?;
            if !authored {
                b.world.catalog.get_mut(barracks_type)?.rally = None;
            }
            let friend = b.world.spawn_unit(b.soldier, PlayerId(0), friend_at)?;
            let target = match target {
                RallyTarget::Unit(_) => RallyTarget::Unit(friend),
                other => other,
            };
            b.set_rally(b.barracks, target)?;
            let (child, _) = b.train(b.barracks, b.soldier)?;
            outcomes.push((b.front_order(child), b.world.entities[&child].position));
        }
        assert_eq!(outcomes[0], outcomes[1], "{target:?}");
        assert!(outcomes[0].0.is_some(), "{target:?}: an order");
    }
    Ok(())
}

/// The cap of waypoints the catalog authors for `producer`'s rally ability.
fn cap_of(b: &Base, producer: EntityId) -> Result<usize, Box<dyn std::error::Error>> {
    let arch = b
        .world
        .catalog
        .get(b.world.entities[&producer].unit_type_id)?;
    Ok(usize::from(
        arch.rally.as_ref().ok_or("no rally ability")?.max_waypoints,
    ))
}

/// P-4 (D-3, B-165, B-215): Shift appends a waypoint until the route holds the cap the row
/// authors; the next Shift-click is refused with `RallyWaypointsFull` and changes nothing, and a
/// click without Shift starts the route again.
#[test]
fn shift_appends_waypoints_up_to_the_cap() -> TestResult {
    let mut b = Base::new()?;
    let cap = cap_of(&b, b.barracks)?;
    assert!(cap > 1, "a cap that allows waypoints");
    let targets: Vec<RallyTarget> = (0..=cap)
        .zip(0_u32..)
        .map(|(_, i)| RallyTarget::Point(at(10.5 + 2.0 * f64::from(i), 26.5)))
        .collect();
    for (i, &target) in targets.iter().enumerate().take(cap) {
        assert_eq!(
            b.shift_rally(b.barracks, target, i > 0),
            Ok(GameCommandReceipt::Accepted { recipients: 1 })
        );
        assert_eq!(b.waypoints(b.barracks), targets[..=i]);
    }
    assert_eq!(
        b.shift_rally(b.barracks, targets[cap], true),
        Err(GameCommandError::RallyWaypointsFull)
    );
    assert_eq!(b.waypoints(b.barracks), targets[..cap]);
    b.shift_rally(b.barracks, targets[cap], false)?;
    assert_eq!(b.waypoints(b.barracks), [targets[cap]]);
    Ok(())
}

/// P-4 (D-3): a rally on the structure itself clears the slot's route, with Shift as without.
#[test]
fn shift_on_the_structure_itself_clears_the_slot() -> TestResult {
    let mut b = Base::new()?;
    b.shift_rally(b.barracks, RallyTarget::Point(at(10.5, 26.5)), false)?;
    b.shift_rally(b.barracks, RallyTarget::Point(at(14.5, 26.5)), true)?;
    assert_eq!(b.waypoints(b.barracks).len(), 2);
    b.shift_rally(b.barracks, RallyTarget::Unit(b.barracks), true)?;
    assert_eq!(b.world.entities[&b.barracks].rallies, []);
    Ok(())
}

/// P-4 (D-5, S10 post 25): a new unit gets one order per waypoint, the first replacing and the
/// rest queued, and walks them in order, arriving at each.
#[test]
fn a_new_unit_walks_its_waypoints_in_order() -> TestResult {
    let mut b = Base::new()?;
    let points = [at(14.5, 26.5), at(22.5, 26.5), at(22.5, 14.5)];
    for (i, &point) in points.iter().enumerate() {
        b.shift_rally(b.barracks, RallyTarget::Point(point), i > 0)?;
    }
    let (child, _) = b.train(b.barracks, b.soldier)?;
    let expected: Vec<UnitOrder> = points
        .iter()
        .map(|&target| UnitOrder::Move { target })
        .collect();
    assert_eq!(b.orders(child), expected);
    let mut reached = Vec::new();
    for _ in 0..1_500 {
        if b.world.entities[&child].order_queue.is_empty() {
            break;
        }
        let pending = b.world.entities[&child].order_queue.len();
        b.tick();
        if b.world.entities[&child].order_queue.len() < pending {
            reached.push(b.world.entities[&child].position);
        }
    }
    assert_eq!(
        reached.len(),
        points.len(),
        "every waypoint reached: {reached:?}"
    );
    for (at, point) in reached.iter().zip(points) {
        assert!(
            (*at - point).length_sq() < SimScalar::ONE,
            "{at:?} at {point:?}"
        );
    }
    Ok(())
}

/// P-4 (D-5, S10 post 25: "end with minerals. Your freshly produced SCV will take the tour"): a
/// worker whose route ends on a field walks the earlier points and then harvests it.
#[test]
fn a_route_ending_on_a_field_leaves_a_worker_harvesting() -> TestResult {
    let mut b = Base::new()?;
    let field = ResourceNodeId(0);
    let tour = at(16.5, 16.5);
    b.shift_rally(b.hall, RallyTarget::Point(tour), false)?;
    b.shift_rally(b.hall, RallyTarget::Node(field), true)?;
    let (child, _) = b.train(b.hall, b.worker)?;
    assert_eq!(
        b.orders(child),
        [
            UnitOrder::Move { target: tour },
            UnitOrder::Harvest { target: field }
        ]
    );
    let mut toured = false;
    for _ in 0..1_500 {
        b.tick();
        let entity = &b.world.entities[&child];
        toured |= (entity.position - tour).length_sq() < SimScalar::ONE;
        if toured && entity.cargo.is_some() {
            return Ok(());
        }
    }
    Err(format!("toured {toured}, never carried a load").into())
}

/// P-4 (D-5, S10 posts 1 and 35): a waypoint on a unit that has died is dropped and the new unit
/// goes on to the next; when every waypoint has died the route is cancelled and the unit gets
/// no order.
#[test]
fn a_dead_waypoint_is_skipped_and_an_all_dead_route_is_cancelled() -> TestResult {
    let mut b = Base::new()?;
    let first = b.world.spawn_unit(b.soldier, PlayerId(0), at(16.5, 16.5))?;
    let second = b.world.spawn_unit(b.soldier, PlayerId(0), at(20.5, 16.5))?;
    b.shift_rally(b.barracks, RallyTarget::Unit(first), false)?;
    b.shift_rally(b.barracks, RallyTarget::Unit(second), true)?;
    b.world.entities.get_mut(&first).ok_or("first")?.hp = SimScalar::ZERO;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert_eq!(
        b.orders(child),
        [UnitOrder::Follow {
            target: second,
            kind: FollowKind::Move
        }],
        "the live waypoint is followed (plan 2026-11-26-follow-order D-9)"
    );
    assert_eq!(
        b.waypoints(b.barracks),
        [RallyTarget::Unit(second)],
        "pruned"
    );
    b.world.entities.get_mut(&second).ok_or("second")?.hp = SimScalar::ZERO;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert_eq!(b.orders(child), []);
    assert_eq!(b.world.entities[&b.barracks].rallies, [], "cancelled");
    Ok(())
}

/// P-4 (D-5): a waypoint whose order admission refuses (a point too near the map's corner for
/// the unit's terrain radius) is skipped, so the next waypoint's order is the first issued.
#[test]
fn a_refused_waypoint_order_is_skipped() -> TestResult {
    let mut b = Base::new()?;
    let corner = at(0.0, 0.0);
    let next = at(14.5, 26.5);
    b.shift_rally(b.barracks, RallyTarget::Point(corner), false)?;
    b.shift_rally(b.barracks, RallyTarget::Point(next), true)?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert_eq!(b.orders(child), [UnitOrder::Move { target: next }]);
    Ok(())
}

/// P-5 (D-6, S4: the army rally's validator accepts enemy targets): a rally on another player's
/// unit stores the unit, not a point on the ground.
#[test]
fn a_rally_on_an_enemy_is_stored_as_the_unit() -> TestResult {
    let mut b = Base::new()?;
    let enemy = b.world.spawn_unit(b.soldier, PlayerId(1), at(20.5, 26.5))?;
    assert_eq!(
        b.shift_rally(b.barracks, RallyTarget::Unit(enemy), false),
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );
    assert_eq!(b.waypoints(b.barracks), [RallyTarget::Unit(enemy)]);
    Ok(())
}

/// P-5 (D-6, S11: "a … Attack command … is issued to the rallied unit upon creation"): a new
/// unit that can attack its enemy rally target gets an explicit attack on it on its spawn tick.
#[test]
fn a_new_unit_attacks_its_enemy_rally_target() -> TestResult {
    let mut b = Base::new()?;
    let enemy = b.world.spawn_unit(b.soldier, PlayerId(1), at(20.5, 26.5))?;
    b.shift_rally(b.barracks, RallyTarget::Unit(enemy), false)?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert_eq!(b.orders(child), [UnitOrder::attack(enemy)]);
    Ok(())
}

/// Whether a new `product` could attack `target_type`, judged on two spawned entities of a
/// throwaway world by the admission rule itself ([`sim_core::Entity::can_attack`]).
fn could_attack(
    product: UnitTypeId,
    target_type: UnitTypeId,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut probe = Base::new()?;
    let unit = probe
        .world
        .spawn_unit(product, PlayerId(0), at(10.5, 26.5))?;
    let target = probe
        .world
        .spawn_unit(target_type, PlayerId(1), at(12.5, 26.5))?;
    let (unit, target) = (&probe.world.entities[&unit], &probe.world.entities[&target]);
    Ok(unit.can_attack(
        probe.world.catalog.get(unit.unit_type_id)?,
        target,
        probe.world.catalog.get(target.unit_type_id)?,
    ))
}

/// P-5 (D-6, Q0105's default, B-187): a new unit that cannot attack its enemy rally target (a
/// product of a structure without workers whose weapons do not reach the target, found in the
/// catalog by that rule) follows the enemy (B-166's remaining half, fixed by plan
/// `2026-11-26-follow-order` D-9).
/// A producer without workers and a product of it that cannot attack `b`'s soldier (its weapons
/// do not reach it), found in the catalog by that rule, as `(producer, product)`.
fn unattacking_pair(b: &Base) -> Result<(UnitTypeId, UnitTypeId), Box<dyn std::error::Error>> {
    let target_type = b.soldier;
    let faction = b.world.catalog.get(b.soldier)?.faction;
    for producer in b.world.catalog.archetypes().filter(|arch| {
        arch.faction == faction
            && matches!(arch.shape, ShapeKind::BuildingDome { .. })
            && arch.capabilities.auto_rally.is_none()
            && !arch.trains_units.is_empty()
    }) {
        for slug in &producer.trains_units {
            let Some(product) = b.world.catalog.get_by_slug(slug) else {
                continue;
            };
            if matches!(product.shape, ShapeKind::BuildingDome { .. })
                || product.capabilities.harvester.is_some()
                || !product.requires_structures.is_empty()
            {
                continue;
            }
            if !could_attack(product.unit_type_id, target_type)? {
                return Ok((producer.unit_type_id, product.unit_type_id));
            }
        }
    }
    Err("no product that cannot attack the soldier".into())
}

#[test]
fn a_unit_that_cannot_attack_follows_the_enemy() -> TestResult {
    let b = Base::new()?;
    let target_type = b.soldier;
    let (producer_type, product) = unattacking_pair(&b)?;
    let mut b = Base::new()?;
    let producer = b.world.spawn_structure(producer_type, PlayerId(0), 20, 4)?;
    // Within the producer's sight: a follow of an enemy its player never saw ends at once (plan
    // `2026-11-26-follow-order` D-5).
    let enemy = b
        .world
        .spawn_unit(target_type, PlayerId(1), at(24.5, 12.5))?;
    b.shift_rally(producer, RallyTarget::Unit(enemy), false)?;
    let (child, _) = b.train(producer, product)?;
    assert_eq!(
        b.orders(child),
        [UnitOrder::Follow {
            target: enemy,
            kind: FollowKind::Move
        }],
        "B-166's remaining half (plan 2026-11-26-follow-order D-9): a follow, not a snapshot"
    );
    Ok(())
}

/// P-5 (D-5, D-6, S11: "The rally points are deleted if the target unit dies"): an enemy rally
/// target that dies is pruned at the next spawn, and the new unit gets no order.
#[test]
fn an_enemy_rally_target_that_dies_is_pruned() -> TestResult {
    let mut b = Base::new()?;
    let enemy = b.world.spawn_unit(b.soldier, PlayerId(1), at(20.5, 26.5))?;
    b.shift_rally(b.barracks, RallyTarget::Unit(enemy), false)?;
    b.world.entities.get_mut(&enemy).ok_or("enemy")?.hp = SimScalar::ZERO;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert_eq!(b.orders(child), []);
    assert_eq!(b.world.entities[&b.barracks].rallies, []);
    Ok(())
}

/// P-6: rallies end to end through `TickBundle`s only. Setup places the structures and an enemy
/// structure; every rally and every production job is a `PlayerCommand`.
/// - The Hatchery analogue: a right click on the ground sets its army rally (its worker rally
///   holds its auto-rally's field). It trains a worker, which harvests the field, the
///   non-worker of the army slot, which walks to the ground point, and the excluded product
///   (the Queen analogue), which gets no order.
/// - The Barracks analogue: a right click on the ground, then a Shift-right-click on the enemy
///   structure. A soldier trained before the Shift-click walks to the point; one trained after
///   walks to the point and then attacks.
///
/// Each child's order queue is checked on its spawn tick, and 200 ticks after the last spawn the
/// worker carries or gathers, the soldier attacks, and the Queen analogue stands idle where it
/// left.
#[test]
fn rallies_through_player_commands() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let enemy_type = h.base.world.entities[&h.base.barracks].unit_type_id;
    let enemy = h
        .base
        .world
        .spawn_structure(enemy_type, PlayerId(1), 4, 27)?;
    let (hatch, barracks) = (h.hatch, h.base.barracks);
    let (army_point, soldier_point) = (at(12.5, 26.5), at(14.5, 22.5));
    let command = |entities: Vec<EntityId>, order: UnitOrder, queue: bool| PlayerCommand {
        player_id: PlayerId(0),
        entities,
        order,
        queue,
    };
    let rally = |target| UnitOrder::SetRally { target, slot: None };
    let produce = |unit_type| UnitOrder::Produce { unit_type };
    h.base.tick_with(vec![
        command(vec![hatch], rally(RallyTarget::Point(army_point)), false),
        command(
            vec![barracks],
            rally(RallyTarget::Point(soldier_point)),
            false,
        ),
        command(vec![hatch], produce(h.worker), false),
        command(vec![hatch], produce(h.army), false),
        command(vec![hatch], produce(h.excluded), false),
        command(vec![barracks], produce(h.base.soldier), false),
    ]);
    let outcomes = h.base.world.game_command_outcomes().to_vec();
    assert!(outcomes.iter().all(|o| o.result.is_ok()), "{outcomes:?}");
    // Every spawn: the child's type and its orders on its spawn tick.
    let mut spawned: Vec<(EntityId, UnitTypeId, Vec<UnitOrder>)> = Vec::new();
    let mut shifted = false;
    for _ in 0..6_000 {
        let next = h.base.world.next_entity_id;
        if !shifted && spawned.iter().any(|(_, kind, _)| *kind == h.base.soldier) {
            h.base.tick_with(vec![
                command(vec![barracks], rally(RallyTarget::Unit(enemy)), true),
                command(vec![barracks], produce(h.base.soldier), false),
            ]);
            shifted = true;
        } else {
            h.base.tick();
        }
        for id in next..h.base.world.next_entity_id {
            let id = EntityId(id);
            if let Some(entity) = h.base.world.entities.get(&id) {
                spawned.push((id, entity.unit_type_id, h.base.orders(id)));
            }
        }
        if spawned.len() == 5 {
            break;
        }
    }
    let of = |kind: UnitTypeId| spawned.iter().filter(move |(_, k, _)| *k == kind);
    let field = UnitOrder::Harvest {
        target: ResourceNodeId(0),
    };
    let (worker, _, orders) = of(h.worker).next().ok_or("no worker")?.clone();
    assert_eq!(orders, std::slice::from_ref(&field));
    let (_, _, orders) = of(h.army).next().ok_or("no army unit")?.clone();
    assert_eq!(orders, [UnitOrder::Move { target: army_point }]);
    let (queen, _, orders) = of(h.excluded).next().ok_or("no excluded unit")?.clone();
    assert_eq!(orders, []);
    let soldiers: Vec<_> = of(h.base.soldier).cloned().collect();
    assert_eq!(soldiers.len(), 2, "{spawned:?}");
    assert_eq!(
        soldiers[0].2,
        [UnitOrder::Move {
            target: soldier_point
        }]
    );
    assert_eq!(
        soldiers[1].2,
        [
            UnitOrder::Move {
                target: soldier_point
            },
            UnitOrder::attack(enemy)
        ]
    );
    let queen_left = h.base.world.entities[&queen].position;
    for _ in 0..200 {
        h.base.tick();
    }
    let world = &h.base.world;
    let worker = &world.entities[&worker];
    assert!(
        worker.cargo.is_some() || worker.order_queue.front() == Some(&field),
        "the worker harvests: {:?}",
        worker.order_queue
    );
    let soldier = &world.entities[&soldiers[1].0];
    assert_eq!(soldier.order_queue.front(), Some(&UnitOrder::attack(enemy)));
    let queen = &world.entities[&queen];
    assert!(queen.order_queue.is_empty());
    assert_eq!(queen.position, queen_left);
    Ok(())
}

/// Plan `2026-11-26-follow-order` P-9 (D-9): a rally on one's own structure stays a point move to
/// where it stands, a structure being no body to follow.
#[test]
fn a_rally_on_an_own_structure_stays_a_point_move() -> TestResult {
    let mut b = Base::new()?;
    b.set_rally(b.barracks, RallyTarget::Unit(b.hall))?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    assert!(
        matches!(b.front_order(child), Some(UnitOrder::Move { .. })),
        "{:?}",
        b.front_order(child)
    );
    Ok(())
}

/// Plan `2026-11-26-follow-order` P-9 (D-5, D-9; S11): a route of two own units is visited in
/// turn: the new unit follows the first until it reaches it, then follows the second.
#[test]
fn a_route_of_two_own_units_is_followed_in_turn() -> TestResult {
    let mut b = Base::new()?;
    let first = b.world.spawn_unit(b.soldier, PlayerId(0), at(14.5, 26.5))?;
    let second = b.world.spawn_unit(b.soldier, PlayerId(0), at(24.5, 28.5))?;
    b.shift_rally(b.barracks, RallyTarget::Unit(first), false)?;
    b.shift_rally(b.barracks, RallyTarget::Unit(second), true)?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    let follow = |target| UnitOrder::Follow {
        target,
        kind: FollowKind::Move,
    };
    assert_eq!(b.orders(child), [follow(first), follow(second)]);
    for _ in 0..600 {
        b.tick();
        let entity = &b.world.entities[&child];
        if entity.order_queue.len() == 1 && entity.movement.follow_rest.is_some() {
            assert_eq!(b.orders(child), [follow(second)]);
            let gap = (entity.position - b.world.entities[&second].position).length_sq();
            assert!(gap < SimScalar::from_num(4), "it rests by the second unit");
            return Ok(());
        }
    }
    Err("the new unit never rested by the second unit".into())
}

/// Plan `2026-11-26-follow-order` P-9 (D-5; S9, S10): a new unit following its rally unit stops
/// when that unit dies.
#[test]
fn a_new_unit_following_its_rally_unit_stops_when_it_dies() -> TestResult {
    let mut b = Base::new()?;
    let friend = b.world.spawn_unit(b.soldier, PlayerId(0), at(24.5, 28.5))?;
    b.set_rally(b.barracks, RallyTarget::Unit(friend))?;
    let (child, _) = b.train(b.barracks, b.soldier)?;
    for _ in 0..10 {
        b.tick();
    }
    assert!(matches!(
        b.front_order(child),
        Some(UnitOrder::Follow { .. })
    ));
    b.world.entities.get_mut(&friend).ok_or("friend")?.hp = SimScalar::ZERO;
    b.tick();
    b.tick();
    assert_eq!(b.orders(child), []);
    Ok(())
}

/// P-4, B-217 (plan `2026-12-01-rallies-and-smart-command` D-4, §2.2 rule 4): the mirror of
/// [`the_queen_analogue_uses_no_rally`]. With the worker rally cleared (a rally on the structure
/// itself) and only the army rally set, a new worker gets no order: it is refused by the army
/// slot's use rule (`-;Worker`), and nothing falls through to it.
#[test]
fn a_worker_gets_no_order_while_only_the_army_rally_is_set() -> TestResult {
    let mut h = TwoSlotBase::new()?;
    let hatch = RallyTarget::Unit(h.hatch);
    h.set(hatch, Some(h.worker_slot))?;
    let ground = RallyTarget::Point(at(16.5, 26.5));
    h.set(ground, Some(h.army_slot))?;
    assert_eq!(h.routes(), [RallyRoute::single(h.army_slot, ground)]);
    let (worker, _) = h.base.train(h.hatch, h.worker)?;
    assert_eq!(
        h.base.front_order(worker),
        None,
        "no fall-through to the army rally"
    );
    Ok(())
}

/// One row of P-3's table and the front order its child had on its spawn tick.
type RowFront = (usize, Option<UnitOrder>);

/// P-3's row 9: the barracks' rally is set on `rider`, which then boards `carrier`; the next
/// soldier's front order on its spawn tick. Checks that the waypoint on the carried unit stays.
fn carried_row(
    b: &mut Base,
    rider: EntityId,
    carrier: EntityId,
) -> Result<Option<UnitOrder>, Box<dyn std::error::Error>> {
    let barracks = b.barracks;
    b.set_rally(barracks, RallyTarget::Unit(rider))?;
    b.command(vec![rider], UnitOrder::Load { carrier })?;
    for _ in 0..400 {
        if b.world.entities[&rider].is_loaded_in_transport() {
            break;
        }
        b.tick();
    }
    assert!(
        b.world.entities[&rider].is_loaded_in_transport(),
        "the rider boarded"
    );
    let (carried, _) = b.train(barracks, b.soldier)?;
    assert_eq!(
        b.waypoints(barracks),
        [RallyTarget::Unit(rider)],
        "the waypoint on a carried unit stays"
    );
    Ok(b.front_order(carried))
}

/// P-3's rows 8 and 7, trained in that order, on an enemy soldier within the sight of a second
/// producer whose `follower` cannot attack it (so the follow does not end at once, plan
/// `2026-11-26-follow-order` D-5); the enemy and the two `(row, front order)` pairs.
fn enemy_rows(
    b: &mut Base,
    follower_producer: UnitTypeId,
    follower: UnitTypeId,
) -> Result<(EntityId, Vec<RowFront>), Box<dyn std::error::Error>> {
    let producer = b
        .world
        .spawn_structure(follower_producer, PlayerId(0), 20, 4)?;
    let enemy = b.world.spawn_unit(b.soldier, PlayerId(1), at(24.5, 12.5))?;
    let mut rows = Vec::new();
    for (index, producer, product) in [(8, producer, follower), (7, b.barracks, b.soldier)] {
        b.shift_rally(producer, RallyTarget::Unit(enemy), false)?;
        let (child, _) = b.train(producer, product)?;
        rows.push((index, b.front_order(child)));
    }
    Ok((enemy, rows))
}

/// P-3 (B-163's record; plan `2026-12-01-rallies-and-smart-command` WP2): one constructed base
/// gives a new unit each row of `give_rally_order`'s documented table, in its order, and the
/// child's front order on its spawn tick is the row's:
/// 1. a point: `Move` to it;
/// 2. a node, the child a harvester: `Harvest`;
/// 3. a node, the child no harvester: `Move` to the node;
/// 4. one's own carrier: `Load`;
/// 5. one's own mobile unit: `Follow { Move }`;
/// 6. one's own structure: `Move` to it;
/// 7. another player's unit the child can attack: `AttackTarget { Explicit }`;
/// 8. another player's mobile unit it cannot attack: `Follow { Move }`;
/// 9. a carried unit: no order, and the waypoint stays.
///
/// The enemy rows are trained last, the followed one first, so the attacked enemy is alive when
/// both children appear.
#[test]
fn the_rally_order_table_is_as_documented() -> TestResult {
    let (follower_producer, follower) = unattacking_pair(&Base::new()?)?;
    let mut b = Base::new()?;
    let carrier_type = b
        .world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.transport_capacity > 0)
        .ok_or("no carrier in the catalog")?
        .unit_type_id;
    let carrier = b
        .world
        .spawn_unit(carrier_type, PlayerId(0), at(12.5, 22.5))?;
    let friend = b.world.spawn_unit(b.soldier, PlayerId(0), at(16.5, 16.5))?;
    let rider = b.world.spawn_unit(b.soldier, PlayerId(0), at(12.5, 25.5))?;
    let (barracks, soldier, hall, worker) = (b.barracks, b.soldier, b.hall, b.worker);
    let point = at(14.5, 26.5);
    let field = ResourceNodeId(0);
    let mut fronts: Vec<RowFront> = Vec::new();
    let mut row = |b: &mut Base,
                   index: usize,
                   producer: EntityId,
                   product: UnitTypeId,
                   target: RallyTarget|
     -> Result<EntityId, Box<dyn std::error::Error>> {
        b.set_rally(producer, target)?;
        let (child, _) = b.train(producer, product)?;
        fronts.push((index, b.front_order(child)));
        Ok(child)
    };
    row(&mut b, 1, barracks, soldier, RallyTarget::Point(point))?;
    row(&mut b, 2, hall, worker, RallyTarget::Node(field))?;
    let walker = row(&mut b, 3, barracks, soldier, RallyTarget::Node(field))?;
    b.assert_walks_to_node(walker, field.0)?;
    row(&mut b, 4, barracks, soldier, RallyTarget::Unit(carrier))?;
    row(&mut b, 5, barracks, soldier, RallyTarget::Unit(friend))?;
    row(&mut b, 6, barracks, soldier, RallyTarget::Unit(hall))?;
    fronts.push((9, carried_row(&mut b, rider, carrier)?));
    let (enemy, enemy_rows) = enemy_rows(&mut b, follower_producer, follower)?;
    fronts.extend(enemy_rows);
    fronts.sort_by_key(|(index, _)| *index);

    let hall_at = b.world.entities[&hall].position;
    let follow = |target| {
        Some(UnitOrder::Follow {
            target,
            kind: FollowKind::Move,
        })
    };
    let walked_to_hall = |order: &Option<UnitOrder>| {
        // The hall's centre lies inside its footprint, which movement admission moves to the
        // nearest point a body can stand on (S-4): within its half diagonal and a body.
        matches!(order, Some(UnitOrder::Move { target })
            if (*target - hall_at).length_sq() < SimScalar::from_num(36))
    };
    assert_eq!(fronts.len(), 9, "one child per row: {fronts:?}");
    assert_eq!(
        fronts[0].1,
        Some(UnitOrder::Move { target: point }),
        "row 1"
    );
    assert_eq!(
        fronts[1].1,
        Some(UnitOrder::Harvest { target: field }),
        "row 2"
    );
    assert!(
        matches!(fronts[2].1, Some(UnitOrder::Move { .. })),
        "row 3: {:?}",
        fronts[2]
    );
    assert_eq!(fronts[3].1, Some(UnitOrder::Load { carrier }), "row 4");
    assert_eq!(fronts[4].1, follow(friend), "row 5");
    assert!(walked_to_hall(&fronts[5].1), "row 6: {:?}", fronts[5]);
    assert_eq!(fronts[6].1, Some(UnitOrder::attack(enemy)), "row 7");
    assert_eq!(fronts[7].1, follow(enemy), "row 8");
    assert_eq!(fronts[8].1, None, "row 9");
    Ok(())
}
