//! Rally points of production structures (plan `2026-11-23-production-queue` D-3 to D-7; B-60).
//!
//! `SetRally` admission and the new unit's order (D-3, D-4) are checked on a constructed base whose
//! producers and products are read from the shipped catalog by capability. The town hall's
//! auto-rally is checked on the shipped showcase against a node computed here
//! from the map's nodes and the archetype's authored `auto_rally`, and on constructed maps for
//! the cases the showcase does not reach. No identity is named: the hall is whichever archetype
//! carries the capability.

use sim_core::economy::{GameCommandError, GameCommandReceipt};
use sim_core::scenario::load_scenario_world;
use sim_core::{SimWorld, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
use sim_types::{
    AutoRallySpec, EntityId, PlayerCommand, PlayerId, PlayerSlot, RallyRoute, RallyTarget,
    ResourceNodeId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
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
/// selection with none is refused, as are a point off the map, a unit that is another
/// player's, dead or absent, and a node not on the map. Refusals change nothing.
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
    let foreign = b.world.spawn_unit(b.worker, PlayerId(1), point(12, 14))?;
    let dead = b.world.spawn_unit(b.worker, PlayerId(0), point(12, 16))?;
    b.world.entities.get_mut(&dead).ok_or("dead absent")?.hp = SimScalar::ZERO;
    let off = SimVec2::new(SimScalar::from_i32(33), SimScalar::from_i32(4));
    for (target, refusal) in [
        (
            RallyTarget::Point(off),
            GameCommandError::RallyOutsideMap(off),
        ),
        (
            RallyTarget::Unit(foreign),
            GameCommandError::InvalidRallyTarget(foreign),
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

/// D-4, B-163: a unit rallied to a friend goes to where the friend stood when it left the
/// structure, even though the friend walks on (the game's unit follows; registered).
#[test]
fn a_unit_rallied_to_a_friend_goes_where_the_friend_stood() -> TestResult {
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
    assert_eq!(
        b.front_order(child),
        Some(UnitOrder::Move { target: stood })
    );
    b.tick();
    assert_ne!(
        b.world.entities[&friend].position, stood,
        "the friend walks on"
    );
    assert_eq!(
        b.front_order(child),
        Some(UnitOrder::Move { target: stood })
    );
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
