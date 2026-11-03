//! Decides what a right click means, from what is under the pointer and what is selected.
//!
//! # Responsibility
//! Owns the precedence of the right click, and nothing else: [`resolve_target`] chooses among what
//! lies under the pointer, and [`context_order`] answers a resolved [`PointerTarget`] with an order.
//! Picking belongs to [`BattleView`](super::BattleView), and submission to the battle queue.
//!
//! # How it fits
//! Every row asks [`any_recipient`], the rule the queue itself admits members by, so a target
//! never offers an order the queue would then refuse for the whole selection. Mechanics are read
//! as capabilities of the target's archetype, never by its identity, and this module uses no
//! Bevy system, so it is tested without an app.
//!
//! # Invariants
//! - Pure: the world is read and never written, and the same inputs give the same order.
//! - The first row that some selected unit can carry out wins, and the last row, a `Move`, always
//!   applies. An order the simulation then refuses reaches the player through its typed outcome.
//! - Runs on a right click, so its allocation-free O(nodes) scan is never a per-frame cost.

use bevy::math::Vec3;
use sim_core::SimWorld;
use sim_types::{AttackOrigin, EntityId, PlayerId, ResourceNodeId, SimScalar, SimVec2, UnitOrder};

use super::order_marker::OrderMarkerKind;
use crate::battle_pipeline::commands::any_recipient;

/// What a right click landed on. Every point is in Bevy world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerTarget {
    /// A unit of another player, with the ground below it.
    Enemy(EntityId, Vec3),
    /// A unit of the local player, with the ground under the pointer.
    Friendly(EntityId, Vec3),
    /// A resource node, with the ground under the pointer.
    Node(ResourceNodeId, Vec3),
    /// Open ground under the pointer. Neutral units count as ground.
    Ground(Vec3),
}

/// Chooses what a right click means when the pointer ray hit both a unit and a node.
///
/// `unit` and `node` are what picking found, if anything, and a unit wins, so a dome over a gas
/// node stays a dome. The one exception keeps mineral lines workable: where `selection` can harvest
/// the node, a mobile friend standing in the way does not count, or a worker would walk to the spot
/// instead of mining. An enemy is never passed over, and neither is a friendly structure, which
/// has orders of its own: a foundation to resume, a dropoff for cargo, an extractor to work.
#[must_use]
pub fn resolve_target(
    world: &SimWorld,
    owner: PlayerId,
    selection: &[EntityId],
    unit: Option<PointerTarget>,
    node: Option<PointerTarget>,
) -> Option<PointerTarget> {
    match (unit, node) {
        (Some(PointerTarget::Friendly(friend, _)), Some(PointerTarget::Node(target, _)))
            if is_mobile(world, friend)
                && any_recipient(world, owner, selection, &UnitOrder::Harvest { target }) =>
        {
            node
        }
        (Some(_), _) => unit,
        (None, _) => node,
    }
}

/// Whether `id` is a unit rather than a structure.
fn is_mobile(world: &SimWorld, id: EntityId) -> bool {
    world
        .entities
        .get(&id)
        .and_then(|entity| world.catalog.get(entity.unit_type_id).ok())
        .is_some_and(|archetype| !archetype.is_immobile_by_default())
}

/// The order a right click on `target` means for `selection`, first match wins.
///
/// Returns the order, how to mark it, and the Bevy world point the marker belongs at.
///
/// | Target | Condition on the selection | Order |
/// |---|---|---|
/// | Enemy | any | `AttackTarget`, `Explicit` |
/// | Friendly foundation | any can raise it | `Construct` |
/// | Friendly carrier | any can board it, which the carrier itself cannot | `Load` |
/// | Friendly completed dropoff | any holds cargo | `ReturnCargo` |
/// | Friendly completed extractor | any harvester | `Harvest` of the node under it |
/// | Node | any harvester | `Harvest` |
/// | Anything else | | `Move` to the ground under the pointer |
#[must_use]
pub fn context_order(
    world: &SimWorld,
    owner: PlayerId,
    selection: &[EntityId],
    target: PointerTarget,
) -> (UnitOrder, OrderMarkerKind, Vec3) {
    let (special, ground) = match target {
        PointerTarget::Enemy(target, ground) => {
            let order = UnitOrder::AttackTarget {
                target,
                origin: AttackOrigin::Explicit,
            };
            return (order, OrderMarkerKind::Attack, ground);
        }
        PointerTarget::Friendly(id, ground) => {
            (friendly_order(world, owner, selection, id), ground)
        }
        PointerTarget::Node(target, ground) => {
            let harvest = UnitOrder::Harvest { target };
            let order = any_recipient(world, owner, selection, &harvest).then_some(harvest);
            (order, ground)
        }
        PointerTarget::Ground(ground) => (None, ground),
    };
    let order = special.unwrap_or_else(|| UnitOrder::Move {
        target: sim_point(ground),
    });
    (order, OrderMarkerKind::Move, ground)
}

/// Converts a Bevy world point to the simulation's map plane, where Bevy's Z is map Y.
#[must_use]
pub fn sim_point(point: Vec3) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(point.x), SimScalar::from_num(point.z))
}

/// The order `friend`, one of the player's own units, offers `selection`, if any.
fn friendly_order(
    world: &SimWorld,
    owner: PlayerId,
    selection: &[EntityId],
    friend: EntityId,
) -> Option<UnitOrder> {
    let entity = world.entities.get(&friend)?;
    let archetype = world.catalog.get(entity.unit_type_id).ok()?;
    let completed = entity.is_completed;
    [
        entity
            .construction
            .is_some()
            .then_some(UnitOrder::Construct { foundation: friend }),
        (completed && archetype.capabilities.transport_capacity > 0)
            .then_some(UnitOrder::Load { carrier: friend }),
        (completed && archetype.is_resource_dropoff).then_some(UnitOrder::ReturnCargo),
        (completed && archetype.capabilities.is_gas_extractor)
            .then(|| node_under(world, friend))
            .flatten()
            .map(|target| UnitOrder::Harvest { target }),
    ]
    .into_iter()
    .flatten()
    .find(|order| any_recipient(world, owner, selection, order))
}

/// The resource node beneath `structure`, if any.
fn node_under(world: &SimWorld, structure: EntityId) -> Option<ResourceNodeId> {
    world
        .map
        .resources()
        .iter()
        .position(|node| {
            world
                .map
                .cell(node.cell.0, node.cell.1)
                .is_ok_and(|cell| cell.occupying_structure == Some(structure))
        })
        .and_then(|index| u32::try_from(index).ok())
        .map(ResourceNodeId)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::entity::Cargo;
    use sim_core::production::construction::Construction;
    use sim_core::UnitArchetype;
    use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
    use sim_types::{MoverType, ShapeKind, UnitTypeId};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const ME: PlayerId = PlayerId(0);
    const THEM: PlayerId = PlayerId(1);
    const SIDE: u32 = 64;
    /// A 3x3 structure at `EXTRACTOR_CORNER` covers this cell.
    const GAS_CELL: u32 = 41;
    const EXTRACTOR_CORNER: u32 = 40;
    const MINERAL_CELL: u32 = 50;
    const POINT: Vec3 = Vec3::new(12.0, 0.0, 34.0);

    fn node(kind: ResourceKind, cell: u32) -> ResourceNode {
        let at = SimScalar::from_num(cell) + SimScalar::HALF;
        ResourceNode {
            pos: SimVec2::new(at, at),
            kind,
            remaining: 100,
            cell: (cell, cell),
        }
    }

    /// A flat world holding one gas node and one mineral node, in that order.
    fn flat_world() -> Result<SimWorld, Box<dyn std::error::Error>> {
        let map = GameMap::from_cells(
            SIDE,
            SIDE,
            vec![MapCell::default_ground(CliffLevel(0)); (SIDE * SIDE) as usize],
            vec![
                node(ResourceKind::Gas, GAS_CELL),
                node(ResourceKind::Mineral, MINERAL_CELL),
            ],
            Vec::new(),
        )?;
        Ok(sim_fixtures::world(map, 11))
    }

    fn is_dome(archetype: &UnitArchetype) -> bool {
        matches!(archetype.shape, ShapeKind::BuildingDome { .. })
    }

    fn type_where(
        world: &SimWorld,
        wanted: impl Fn(&UnitArchetype) -> bool,
    ) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
        Ok(world
            .catalog
            .archetypes()
            .find(|archetype| wanted(archetype))
            .map(|archetype| archetype.unit_type_id)
            .ok_or("the catalog has no such role")?)
    }

    /// Spawns a mobile unit of `ME`, spaced by `slot`.
    fn unit_where(
        world: &mut SimWorld,
        slot: i32,
        wanted: impl Fn(&UnitArchetype) -> bool,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let unit_type = type_where(world, |a| !is_dome(a) && wanted(a))?;
        let position = SimVec2::new(SimScalar::from_i32(8 + 3 * slot), SimScalar::from_i32(10));
        Ok(world.spawn_unit(unit_type, ME, position)?)
    }

    /// A unit that gathers and raises structures.
    fn worker(world: &mut SimWorld, slot: i32) -> Result<EntityId, Box<dyn std::error::Error>> {
        unit_where(world, slot, |a| {
            a.capabilities.harvester.is_some() && a.capabilities.can_construct
        })
    }

    /// A ground unit that neither gathers nor raises, and carries nothing.
    fn soldier(world: &mut SimWorld, slot: i32) -> Result<EntityId, Box<dyn std::error::Error>> {
        unit_where(world, slot, |a| {
            a.capabilities.harvester.is_none()
                && !a.capabilities.can_construct
                && a.capabilities.transport_capacity == 0
                && a.mover_type == MoverType::Ground
        })
    }

    /// Gives `worker` a load of minerals.
    fn load(world: &mut SimWorld, worker: EntityId) -> TestResult {
        world
            .entities
            .get_mut(&worker)
            .ok_or("worker absent")?
            .cargo = Some(Cargo {
            kind: ResourceKind::Mineral,
            amount: 5,
        });
        Ok(())
    }

    /// Spawns a completed structure of `ME` of the worker's faction that `wanted` accepts.
    fn structure_where(
        world: &mut SimWorld,
        builder: EntityId,
        corner: u32,
        wanted: impl Fn(&UnitArchetype) -> bool,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let builder_type = world.entities.get(&builder).ok_or("absent")?.unit_type_id;
        let faction = world.catalog.get(builder_type)?.faction;
        let structure = type_where(world, |a| is_dome(a) && a.faction == faction && wanted(a))?;
        Ok(world.spawn_structure(structure, ME, corner, corner)?)
    }

    /// Turns a completed structure back into an unfinished foundation.
    fn unfinish(world: &mut SimWorld, id: EntityId) -> TestResult {
        let entity = world.entities.get_mut(&id).ok_or("structure absent")?;
        entity.is_completed = false;
        entity.construction = Some(Construction {
            remaining_ticks: 100,
            total_ticks: 100,
            hp_per_tick: SimScalar::ONE,
            final_hp_residue: SimScalar::ZERO,
            paid_minerals: 0,
            paid_gas: 0,
            last_worked_tick: None,
        });
        Ok(())
    }

    /// The transport role is not on a ground unit in the catalog, so the test authors one.
    fn ground_carrier(
        world: &mut SimWorld,
        slot: i32,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let carrier = unit_where(world, slot, |a| a.capabilities.transport_capacity > 0)?;
        let carrier_type = world.entities.get(&carrier).ok_or("absent")?.unit_type_id;
        world.catalog.get_mut(carrier_type)?.mover_type = MoverType::Ground;
        Ok(carrier)
    }

    fn order_on(
        world: &SimWorld,
        selection: &[EntityId],
        target: PointerTarget,
    ) -> (UnitOrder, OrderMarkerKind) {
        let (order, kind, at) = context_order(world, ME, selection, target);
        assert_eq!(at, POINT, "the marker stays at the resolved point");
        (order, kind)
    }

    fn moved() -> (UnitOrder, OrderMarkerKind) {
        let order = UnitOrder::Move {
            target: sim_point(POINT),
        };
        (order, OrderMarkerKind::Move)
    }

    fn follows(order: UnitOrder) -> (UnitOrder, OrderMarkerKind) {
        (order, OrderMarkerKind::Move)
    }

    #[test]
    fn an_enemy_is_attacked_by_any_selection() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let enemy_type = type_where(&world, |a| !is_dome(a))?;
        let enemy = world.spawn_unit(
            enemy_type,
            THEM,
            SimVec2::new(SimScalar::from_i32(30), SimScalar::from_i32(30)),
        )?;
        assert_eq!(
            order_on(&world, &[unit], PointerTarget::Enemy(enemy, POINT)),
            (UnitOrder::attack(enemy), OrderMarkerKind::Attack)
        );
        Ok(())
    }

    #[test]
    fn a_foundation_is_resumed_only_by_a_unit_that_can_raise_it() -> TestResult {
        let mut world = flat_world()?;
        let builder = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        let foundation = structure_where(&mut world, builder, 20, |_| true)?;
        unfinish(&mut world, foundation)?;
        let target = PointerTarget::Friendly(foundation, POINT);

        assert_eq!(
            order_on(&world, &[other, builder], target),
            follows(UnitOrder::Construct { foundation })
        );
        assert_eq!(order_on(&world, &[other], target), moved());
        Ok(())
    }

    #[test]
    fn an_unfinished_carrier_or_dropoff_offers_only_construction() -> TestResult {
        let mut world = flat_world()?;
        let builder = worker(&mut world, 0)?;
        load(&mut world, builder)?;
        let depot = structure_where(&mut world, builder, 20, |a| a.is_resource_dropoff)?;
        unfinish(&mut world, depot)?;
        let archetype = world.catalog.get(world.entities[&depot].unit_type_id)?;
        assert!(archetype.is_resource_dropoff);

        assert_eq!(
            order_on(&world, &[builder], PointerTarget::Friendly(depot, POINT)),
            follows(UnitOrder::Construct { foundation: depot }),
            "a load cannot go to an unfinished depot"
        );
        Ok(())
    }

    #[test]
    fn a_carrier_is_boarded_by_a_ground_unit() -> TestResult {
        let mut world = flat_world()?;
        let carrier = ground_carrier(&mut world, 0)?;
        let passenger = soldier(&mut world, 1)?;
        let target = PointerTarget::Friendly(carrier, POINT);

        assert_eq!(
            order_on(&world, &[passenger], target),
            follows(UnitOrder::Load { carrier })
        );
        assert_eq!(
            order_on(&world, &[carrier, passenger], target),
            follows(UnitOrder::Load { carrier }),
            "the carrier in the selection does not stop the others"
        );
        Ok(())
    }

    #[test]
    fn a_carrier_alone_does_not_board_itself() -> TestResult {
        let mut world = flat_world()?;
        let carrier = ground_carrier(&mut world, 0)?;
        assert_eq!(
            order_on(&world, &[carrier], PointerTarget::Friendly(carrier, POINT)),
            moved()
        );
        Ok(())
    }

    #[test]
    fn a_dropoff_takes_cargo_and_is_otherwise_ground() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let depot = structure_where(&mut world, unit, 20, |a| a.is_resource_dropoff)?;
        let target = PointerTarget::Friendly(depot, POINT);

        assert_eq!(order_on(&world, &[unit], target), moved());
        load(&mut world, unit)?;
        assert_eq!(
            order_on(&world, &[unit], target),
            follows(UnitOrder::ReturnCargo)
        );
        Ok(())
    }

    #[test]
    fn a_completed_extractor_is_harvested_at_the_node_under_it() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        let extractor = structure_where(&mut world, unit, EXTRACTOR_CORNER, |a| {
            a.capabilities.is_gas_extractor
        })?;
        let target = PointerTarget::Friendly(extractor, POINT);
        let gas = ResourceNodeId(0);

        assert_eq!(
            order_on(&world, &[unit], target),
            follows(UnitOrder::Harvest { target: gas })
        );
        assert_eq!(order_on(&world, &[other], target), moved());
        unfinish(&mut world, extractor)?;
        assert_eq!(
            order_on(&world, &[unit], target),
            follows(UnitOrder::Construct {
                foundation: extractor
            }),
            "an unfinished extractor gives no gas"
        );
        Ok(())
    }

    #[test]
    fn a_node_outranks_a_mobile_friend_for_a_selection_that_can_harvest_it() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        let friend = Some(PointerTarget::Friendly(other, POINT));
        let node = Some(PointerTarget::Node(ResourceNodeId(1), POINT));

        assert_eq!(resolve_target(&world, ME, &[unit], friend, node), node);
        assert_eq!(
            resolve_target(&world, ME, &[other], friend, node),
            friend,
            "a selection that cannot harvest gets nothing from the node"
        );
        Ok(())
    }

    #[test]
    fn a_node_never_outranks_an_enemy() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let enemy_type = type_where(&world, |a| !is_dome(a))?;
        let enemy = world.spawn_unit(
            enemy_type,
            THEM,
            SimVec2::new(SimScalar::from_i32(30), SimScalar::from_i32(30)),
        )?;
        let target = Some(PointerTarget::Enemy(enemy, POINT));
        let node = Some(PointerTarget::Node(ResourceNodeId(1), POINT));

        assert_eq!(resolve_target(&world, ME, &[unit], target, node), target);
        Ok(())
    }

    #[test]
    fn a_node_never_outranks_a_friendly_structure() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let foundation = structure_where(&mut world, unit, EXTRACTOR_CORNER, |a| {
            a.capabilities.is_gas_extractor
        })?;
        unfinish(&mut world, foundation)?;
        let target = Some(PointerTarget::Friendly(foundation, POINT));
        let node = Some(PointerTarget::Node(ResourceNodeId(0), POINT));

        assert_eq!(
            resolve_target(&world, ME, &[unit], target, node),
            target,
            "an unfinished extractor over its node must stay resumable"
        );
        Ok(())
    }

    #[test]
    fn with_nothing_to_choose_between_what_was_hit_stands() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let node = Some(PointerTarget::Node(ResourceNodeId(1), POINT));
        assert_eq!(resolve_target(&world, ME, &[unit], None, node), node);
        assert_eq!(resolve_target(&world, ME, &[unit], None, None), None);
        Ok(())
    }

    #[test]
    fn a_node_is_harvested_by_a_harvester_and_walked_to_by_the_rest() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        let mineral = ResourceNodeId(1);
        let target = PointerTarget::Node(mineral, POINT);

        assert_eq!(
            order_on(&world, &[other, unit], target),
            follows(UnitOrder::Harvest { target: mineral })
        );
        assert_eq!(order_on(&world, &[other], target), moved());
        Ok(())
    }

    #[test]
    fn open_ground_and_an_idle_friend_mean_move() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        assert_eq!(
            order_on(&world, &[unit], PointerTarget::Ground(POINT)),
            moved()
        );
        assert_eq!(
            order_on(&world, &[unit], PointerTarget::Friendly(other, POINT)),
            moved()
        );
        Ok(())
    }

    #[test]
    fn a_unit_of_another_player_cannot_make_a_row_match() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        world.entities.get_mut(&unit).ok_or("absent")?.owner = THEM;
        assert_eq!(
            order_on(
                &world,
                &[unit],
                PointerTarget::Node(ResourceNodeId(1), POINT)
            ),
            moved()
        );
        Ok(())
    }

    #[test]
    fn boarding_beats_returning_cargo_at_a_carrier_that_is_also_a_dropoff() -> TestResult {
        let mut world = flat_world()?;
        let carrier = ground_carrier(&mut world, 0)?;
        let carrier_type = world.entities[&carrier].unit_type_id;
        world.catalog.get_mut(carrier_type)?.is_resource_dropoff = true;
        let unit = worker(&mut world, 1)?;
        load(&mut world, unit)?;

        assert_eq!(
            order_on(&world, &[unit], PointerTarget::Friendly(carrier, POINT)),
            follows(UnitOrder::Load { carrier })
        );
        Ok(())
    }

    #[test]
    fn sim_point_reads_bevy_z_as_map_y() {
        assert_eq!(
            sim_point(Vec3::new(1.5, 9.0, 2.5)),
            SimVec2::new(SimScalar::from_num(1.5), SimScalar::from_num(2.5))
        );
    }
}
