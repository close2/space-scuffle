//! Decides what a right click means, from what is under the pointer and what is selected.
//!
//! # Responsibility
//! Owns the precedence of the right click, and nothing else: [`resolve_target`] chooses among what
//! lies under the pointer, and [`smart_plan`] answers a resolved [`PointerTarget`] member by
//! member with a [`SmartPlan`]. Picking belongs to [`BattleView`](super::BattleView), and
//! submission to the battle queue ([`BattleCommandQueue::submit_smart`]).
//!
//! # How it fits
//! `StarCraft` II 5.0.14 resolves the smart command per unit (plan
//! `2026-12-01-rallies-and-smart-command` D-1, B-222): each unit uses the first of its own
//! abilities that takes the target, and falls back to its move ability. Every row asks
//! `takes`, the member rule the queue itself admits members by, so a plan never names a member
//! the queue would then refuse. Mechanics are read as capabilities of the target's archetype,
//! never by its identity, and this module uses no Bevy system, so it is tested without an app.
//!
//! # Invariants
//! - Pure: the world is read and never written, and the same inputs give the same plan.
//! - Each member takes the first row it can carry out; a click gives one command per distinct
//!   order. The entries' recipients are disjoint, in selection order, and the entries come in
//!   the table's row order. A member that takes no row (a structure, or the clicked unit itself)
//!   is in no entry. An order the simulation then refuses reaches the player through its typed
//!   outcome.
//! - Runs on a right click, so its allocation and its O(selection + nodes) scan are never a
//!   per-frame cost.

use bevy::math::Vec3;
use sim_core::movement::MovementLayer;
use sim_core::SimWorld;
use sim_types::{
    AttackOrigin, EntityId, FollowKind, PlayerId, RallyTarget, ResourceNodeId, SimScalar, SimVec2,
    UnitOrder,
};

use super::order_marker::OrderMarkerKind;
#[cfg(doc)]
use crate::battle_pipeline::commands::BattleCommandQueue;
use crate::battle_pipeline::commands::{any_recipient, order_subject, takes};

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

/// What one right click means for a selection, member by member (B-222; `StarCraft` II 5.0.14
/// resolves the smart command per unit, plan `2026-12-01-rallies-and-smart-command` D-1).
#[derive(Debug, Clone, PartialEq)]
pub struct SmartPlan {
    /// One entry per distinct order, in the D-1 table's row order; recipients in selection order,
    /// disjoint across entries, never empty.
    pub orders: Vec<SmartOrder>,
    /// Where the click's marker shows, in Bevy world space: the ground under the target.
    pub marker_at: Vec3,
}

/// One order of a [`SmartPlan`] and the members that take it.
#[derive(Debug, Clone, PartialEq)]
pub struct SmartOrder {
    /// The order, as the simulation admits it.
    pub order: UnitOrder,
    /// The members of the selection that carry it out.
    pub recipients: Vec<EntityId>,
}

impl SmartPlan {
    /// How the click is marked and where, in Bevy world space: an attack marker when the first
    /// entry is an `AttackTarget`, a move marker otherwise, at [`Self::marker_at`].
    #[must_use]
    pub fn marker(&self) -> (OrderMarkerKind, Vec3) {
        let kind = match self.orders.first() {
            Some(SmartOrder {
                order: UnitOrder::AttackTarget { .. },
                ..
            }) => OrderMarkerKind::Attack,
            _ => OrderMarkerKind::Move,
        };
        (kind, self.marker_at)
    }
}

/// The plan of a right click on `target` for `selection`: each member takes the first row of the
/// target's table that it can carry out (the queue's member rule), and members that take the same row share
/// one entry.
///
/// `ground` is the point of the pointer target, in Bevy world space: the ground below an enemy,
/// or under the pointer otherwise.
///
/// | Target | Rows, in order (the first the member takes wins) |
/// |---|---|
/// | Enemy unit `e` | `AttackTarget { e, Explicit }`; else `Follow { e, Move }` when `e` is mobile; else `Move` to the ground below it |
/// | Friendly foundation `f` | `Construct { f }`; else `Move` to the ground |
/// | Friendly completed carrier `c` | `Load { c }`; else `Follow { c, Move }`; nothing for `c` itself |
/// | Friendly completed dropoff | `ReturnCargo` (a member holding cargo); else `Move` to the ground |
/// | Friendly completed extractor on node `n` | `Harvest { n }`; else `Move` to the ground |
/// | Friendly mobile ground unit `u` | `LoadUnit { u }` (a carrier member); else `Follow { u, Move }`; nothing for `u` itself |
/// | Friendly mobile air unit `u` | `Follow { u, Move }`; nothing for `u` itself |
/// | Any other friendly structure | `Move` to the ground |
/// | Node `n` | `Harvest { n }`; else `Move` to the ground |
/// | Ground | `Move` to the ground |
///
/// A friend that is several of these (a carrier that is also a dropoff) offers its rows in the
/// order `Construct`, `Load`, `ReturnCargo`, `Harvest`, `LoadUnit`, `Follow`, and a structure then
/// the ground `Move`. A carrier's right click on one of its owner's walkers loads it (5.0.14
/// `CAbilTransport` `AllowSmartCmd 1`, `core` AbilData:1192-1204; plan
/// `2026-12-03-movement-work-on-solver` WP-T1, B-247), before the move ability's follow, as
/// `MedivacTransport` comes before `move` in the Medivac's abilities (rallies plan S5). A right click never attacks one's own unit (5.0.14's attack ability has
/// `SmartFilters "-;Player,Ally"`, `core` AbilData:115-119). The move ability takes the smart
/// command on anything (`core` AbilData:533-561): on a unit it follows, on the ground it moves.
#[must_use]
pub fn smart_plan(
    world: &SimWorld,
    owner: PlayerId,
    selection: &[EntityId],
    target: PointerTarget,
) -> SmartPlan {
    let rows = smart_rows(world, target);
    // A row whose subject the queue cannot resolve (an attack on a unit no longer available for
    // combat) is taken by no member, as `any_recipient` answers for it.
    let subjects: Vec<_> = rows
        .iter()
        .map(|row| order_subject(world, owner, row).ok())
        .collect();
    let mut recipients: Vec<Vec<EntityId>> = vec![Vec::new(); rows.len()];
    for &member in selection {
        let first = rows.iter().zip(&subjects).position(|(row, subject)| {
            subject.is_some_and(|subject| takes(world, owner, member, row, subject))
        });
        if let Some(index) = first {
            recipients[index].push(member);
        }
    }
    let orders = rows
        .into_iter()
        .zip(recipients)
        .filter(|(_, recipients)| !recipients.is_empty())
        .map(|(order, recipients)| SmartOrder { order, recipients })
        .collect();
    SmartPlan {
        orders,
        marker_at: target.ground(),
    }
}

/// The rows of [`smart_plan`]'s table for `target`, in order.
fn smart_rows(world: &SimWorld, target: PointerTarget) -> Vec<UnitOrder> {
    match target {
        PointerTarget::Enemy(enemy, ground) => {
            let mut rows = vec![UnitOrder::AttackTarget {
                target: enemy,
                origin: AttackOrigin::Explicit,
            }];
            if is_mobile(world, enemy) {
                rows.push(UnitOrder::Follow {
                    target: enemy,
                    kind: FollowKind::Move,
                });
            }
            rows.push(move_to(ground));
            rows
        }
        PointerTarget::Friendly(friend, ground) => friendly_rows(world, friend, ground),
        PointerTarget::Node(node, ground) => {
            vec![UnitOrder::Harvest { target: node }, move_to(ground)]
        }
        PointerTarget::Ground(ground) => vec![move_to(ground)],
    }
}

/// A `Move` to `ground`, a Bevy world point.
fn move_to(ground: Vec3) -> UnitOrder {
    UnitOrder::Move {
        target: sim_point(ground),
    }
}

impl PointerTarget {
    /// The Bevy world point on the ground the target stands on or covers, where its order marker
    /// shows.
    #[must_use]
    pub const fn ground(self) -> Vec3 {
        match self {
            Self::Enemy(_, ground)
            | Self::Friendly(_, ground)
            | Self::Node(_, ground)
            | Self::Ground(ground) => ground,
        }
    }
}

/// The rally a right click on `target` sets for the selection's production structures (plans
/// `2026-11-23-production-queue` D-10 and `2026-11-25-rallies` D-8): the ground at a point, a
/// node, an enemy unit (B-166: the new unit attacks it), and one of the player's units (the
/// structure itself clears its rally). A completed gas extractor of `owner`'s standing on a node
/// rallies to that node, the rule `friendly_rows` gives a harvester (B-216: 5.0.14 counts a
/// refinery as a harvestable resource), so new workers harvest there.
#[must_use]
pub fn rally_target(world: &SimWorld, owner: PlayerId, target: PointerTarget) -> RallyTarget {
    match target {
        PointerTarget::Ground(ground) => RallyTarget::Point(sim_point(ground)),
        PointerTarget::Enemy(unit, _) => RallyTarget::Unit(unit),
        PointerTarget::Node(node, _) => RallyTarget::Node(node),
        PointerTarget::Friendly(unit, _) => own_extractor_node(world, owner, unit)
            .map_or(RallyTarget::Unit(unit), RallyTarget::Node),
    }
}

/// The node under `unit` when it is a completed gas extractor of `owner`'s (B-216).
fn own_extractor_node(world: &SimWorld, owner: PlayerId, unit: EntityId) -> Option<ResourceNodeId> {
    let entity = world.entities.get(&unit)?;
    let extractor = entity.owner == owner
        && entity.is_completed
        && world
            .catalog
            .get(entity.unit_type_id)
            .is_ok_and(|archetype| archetype.capabilities.is_gas_extractor);
    extractor.then(|| node_under(world, unit)).flatten()
}

/// Converts a Bevy world point to the simulation's map plane, where Bevy's Z is map Y.
#[must_use]
pub fn sim_point(point: Vec3) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(point.x), SimScalar::from_num(point.z))
}

/// The rows `friend`, one of the player's own units, offers, in order. A completed mobile walker
/// offers a carrier its pickup (`LoadUnit`) before the follow. The follow of a mobile friend is
/// its last row (plan `2026-11-26-follow-order` D-10; 5.0.14's smart command on one's
/// own unit is a `move` on it), and a structure ends with the move to `ground`, a Bevy world
/// point: a carrier still loads and an extractor still harvests first. A friend that is gone or
/// has no archetype offers only the move.
fn friendly_rows(world: &SimWorld, friend: EntityId, ground: Vec3) -> Vec<UnitOrder> {
    let Some((entity, archetype)) = world.entities.get(&friend).and_then(|entity| {
        world
            .catalog
            .get(entity.unit_type_id)
            .ok()
            .map(|archetype| (entity, archetype))
    }) else {
        return vec![move_to(ground)];
    };
    let completed = entity.is_completed;
    let mobile = !archetype.is_immobile_by_default();
    let walker = MovementLayer::from_mover(archetype.mover_type) == MovementLayer::Ground;
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
        (completed && mobile && walker).then_some(UnitOrder::LoadUnit { passenger: friend }),
        mobile.then_some(UnitOrder::Follow {
            target: friend,
            kind: FollowKind::Move,
        }),
        (!mobile).then(|| move_to(ground)),
    ]
    .into_iter()
    .flatten()
    .collect()
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
    use crate::battle_pipeline::commands::{
        is_armed, BattleCommandQueue, SessionGeneration, SubmissionError,
    };
    use sim_core::entity::Cargo;
    use sim_core::production::construction::Construction;
    use sim_core::UnitArchetype;
    use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
    use sim_types::{AttackCommand, MoverType, ShapeKind, UnitTypeId};

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
            placed: None,
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

    /// The plan of a right click on `target`, as `(order, recipients)` pairs, after checking
    /// the plan's invariants ([`assert_well_formed`]) and that the marker stays at the point.
    fn plan_on(
        world: &SimWorld,
        selection: &[EntityId],
        target: PointerTarget,
    ) -> Vec<(UnitOrder, Vec<EntityId>)> {
        let plan = smart_plan(world, ME, selection, target);
        assert_eq!(
            plan.marker_at, POINT,
            "the marker stays at the resolved point"
        );
        assert_well_formed(world, selection, target, &plan);
        plan.orders
            .into_iter()
            .map(|entry| (entry.order, entry.recipients))
            .collect()
    }

    /// D-2's exclusivity: every recipient's entry is the first row of the target's table it
    /// takes, so the recipient sets are disjoint and their union is exactly the members some row
    /// accepts; recipients keep selection order; entries come in row order and are never empty.
    fn assert_well_formed(
        world: &SimWorld,
        selection: &[EntityId],
        target: PointerTarget,
        plan: &SmartPlan,
    ) {
        let rows = smart_rows(world, target);
        let first_row = |member: EntityId| {
            rows.iter().position(|row| {
                order_subject(world, ME, row)
                    .is_ok_and(|subject| takes(world, ME, member, row, subject))
            })
        };
        let mut expected: Vec<(usize, Vec<EntityId>)> = Vec::new();
        for &member in selection {
            if let Some(index) = first_row(member) {
                match expected.iter_mut().find(|(row, _)| *row == index) {
                    Some((_, members)) => members.push(member),
                    None => expected.push((index, vec![member])),
                }
            }
        }
        expected.sort_by_key(|(row, _)| *row);
        let expected: Vec<(UnitOrder, Vec<EntityId>)> = expected
            .into_iter()
            .map(|(row, members)| (rows[row].clone(), members))
            .collect();
        let actual: Vec<(UnitOrder, Vec<EntityId>)> = plan
            .orders
            .iter()
            .map(|entry| (entry.order.clone(), entry.recipients.clone()))
            .collect();
        assert_eq!(
            actual, expected,
            "each member takes its first row, in row order"
        );
        assert!(plan.orders.iter().all(|entry| !entry.recipients.is_empty()));
    }

    fn ground_move() -> UnitOrder {
        UnitOrder::Move {
            target: sim_point(POINT),
        }
    }

    fn follow(target: EntityId) -> UnitOrder {
        UnitOrder::Follow {
            target,
            kind: FollowKind::Move,
        }
    }

    /// Whether `attacker`'s default stance has Attack and a weapon that reaches `target` (the
    /// rule of `Entity::can_attack` for a unit in its default stance).
    fn hits(attacker: &UnitArchetype, target: &UnitArchetype) -> bool {
        attacker.stance_orders(None).attack == AttackCommand::Attack
            && attacker
                .stance_weapon_against(None, target.mover_type)
                .is_some_and(|weapon| weapon.target_flags.intersects(target.attributes))
    }

    /// Spawns a unit of `THEM` of the first type `wanted` accepts.
    fn enemy_where(
        world: &mut SimWorld,
        wanted: impl Fn(&UnitArchetype) -> bool,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let enemy_type = type_where(world, wanted)?;
        let at = SimVec2::new(SimScalar::from_i32(30), SimScalar::from_i32(30));
        Ok(world.spawn_unit(enemy_type, THEM, at)?)
    }

    /// P-1: an air enemy is attacked by the member whose weapon reaches it, and followed by an
    /// unarmed air unit and by a ground-only unit, which cannot attack it (5.0.14's move ability
    /// takes the smart command on a unit as a follow, D-1).
    #[test]
    fn an_enemy_is_attacked_by_those_that_can_and_followed_by_the_rest() -> TestResult {
        let mut world = flat_world()?;
        let enemy = enemy_where(&mut world, |a| {
            !is_dome(a) && a.mover_type == MoverType::Air && !a.is_immobile_by_default()
        })?;
        let air = world
            .catalog
            .get(world.entities[&enemy].unit_type_id)?
            .clone();
        let ground = |a: &UnitArchetype| a.mover_type == MoverType::Ground;
        let armed = unit_where(&mut world, 0, |a| ground(a) && hits(a, &air))?;
        let unarmed = unit_where(&mut world, 1, |a| {
            a.mover_type == MoverType::Air && !is_armed(a) && a.max_speed > SimScalar::ZERO
        })?;
        let ground_only = unit_where(&mut world, 2, |a| {
            ground(a) && is_armed(a) && !hits(a, &air)
        })?;
        let target = PointerTarget::Enemy(enemy, POINT);

        assert_eq!(
            plan_on(&world, &[armed, unarmed, ground_only], target),
            [
                (UnitOrder::attack(enemy), vec![armed]),
                (follow(enemy), vec![unarmed, ground_only]),
            ]
        );
        assert_eq!(
            smart_plan(&world, ME, &[armed, unarmed], target).marker(),
            (OrderMarkerKind::Attack, POINT),
            "a click with an attack shows the attack marker"
        );
        assert_eq!(
            smart_plan(&world, ME, &[unarmed], target).marker(),
            (OrderMarkerKind::Move, POINT),
            "a click that only follows shows the move marker"
        );
        Ok(())
    }

    /// P-1: an enemy structure, which cannot be followed, is attacked by an armed member and
    /// walked to by an unarmed one, to the ground below it.
    #[test]
    fn an_enemy_structure_is_walked_to_by_a_member_that_cannot_attack_it() -> TestResult {
        let mut world = flat_world()?;
        let structure = type_where(&world, |a| is_dome(a) && !a.capabilities.is_gas_extractor)?;
        let enemy = world.spawn_structure(structure, THEM, 30, 30)?;
        let dome = world.catalog.get(structure)?.clone();
        let armed = unit_where(&mut world, 1, |a| hits(a, &dome))?;
        let unarmed = unit_where(&mut world, 2, |a| {
            !is_armed(a) && a.max_speed > SimScalar::ZERO
        })?;
        let target = PointerTarget::Enemy(enemy, POINT);

        assert_eq!(
            plan_on(&world, &[unarmed], target),
            [(ground_move(), vec![unarmed])]
        );
        assert_eq!(
            plan_on(&world, &[unarmed, armed], target),
            [
                (UnitOrder::attack(enemy), vec![armed]),
                (ground_move(), vec![unarmed]),
            ]
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
            plan_on(&world, &[other, builder], target),
            [
                (UnitOrder::Construct { foundation }, vec![builder]),
                (ground_move(), vec![other]),
            ],
            "the builder resumes it and the soldier walks there"
        );
        assert_eq!(
            plan_on(&world, &[other], target),
            [(ground_move(), vec![other])]
        );
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
            plan_on(&world, &[builder], PointerTarget::Friendly(depot, POINT)),
            [(UnitOrder::Construct { foundation: depot }, vec![builder])],
            "a load cannot go to an unfinished depot"
        );
        Ok(())
    }

    /// P-1: the own carrier (the catalog's, an air unit) is boarded by a ground unit and
    /// followed by an air unit, which cannot board; the carrier itself takes nothing (S5, S6).
    #[test]
    fn an_own_carrier_is_boarded_by_ground_units_and_followed_by_air_units() -> TestResult {
        let mut world = flat_world()?;
        let carrier = unit_where(&mut world, 0, |a| a.capabilities.transport_capacity > 0)?;
        let passenger = soldier(&mut world, 1)?;
        let flyer = unit_where(&mut world, 2, |a| {
            a.mover_type == MoverType::Air
                && a.capabilities.transport_capacity == 0
                && a.max_speed > SimScalar::ZERO
        })?;
        let target = PointerTarget::Friendly(carrier, POINT);

        assert_eq!(
            plan_on(&world, &[passenger, flyer, carrier], target),
            [
                (UnitOrder::Load { carrier }, vec![passenger]),
                (follow(carrier), vec![flyer]),
            ]
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
            plan_on(&world, &[passenger], target),
            [(UnitOrder::Load { carrier }, vec![passenger])]
        );
        assert_eq!(
            plan_on(&world, &[carrier, passenger], target),
            [(UnitOrder::Load { carrier }, vec![passenger])],
            "the carrier in the selection does not stop the others, and takes nothing"
        );
        Ok(())
    }

    /// Plan `2026-12-03-movement-work-on-solver` WP-T1 (B-247): a carrier's right click on an own
    /// walker picks it up, before the follow the rest of the selection takes; an own flyer, which
    /// cannot board, is followed by the carrier too; and the walker itself takes nothing.
    #[test]
    fn a_carrier_picks_up_an_own_walker_and_follows_an_own_flyer() -> TestResult {
        let mut world = flat_world()?;
        let carrier = unit_where(&mut world, 0, |a| a.capabilities.transport_capacity > 0)?;
        let friend = soldier(&mut world, 1)?;
        let walker = soldier(&mut world, 2)?;
        let flyer = unit_where(&mut world, 3, |a| {
            a.mover_type == MoverType::Air
                && a.capabilities.transport_capacity == 0
                && a.max_speed > SimScalar::ZERO
        })?;

        assert_eq!(
            plan_on(
                &world,
                &[friend, carrier, walker],
                PointerTarget::Friendly(walker, POINT)
            ),
            [
                (UnitOrder::LoadUnit { passenger: walker }, vec![carrier]),
                (follow(walker), vec![friend]),
            ]
        );
        assert_eq!(
            plan_on(&world, &[carrier], PointerTarget::Friendly(flyer, POINT)),
            [(follow(flyer), vec![carrier])],
            "a flyer is no passenger"
        );
        world
            .entities
            .get_mut(&walker)
            .ok_or("walker")?
            .is_completed = false;
        assert_eq!(
            plan_on(&world, &[carrier], PointerTarget::Friendly(walker, POINT)),
            [(follow(walker), vec![carrier])],
            "an unfinished unit offers no pickup"
        );
        Ok(())
    }

    /// Plan `2026-11-26-follow-order` D-10 (P-11) and this plan's D-1: a mobile friend that
    /// offers nothing else is followed by the rest of the selection; the friend itself takes no
    /// row, so alone it is given nothing (its smart command on itself has no target to take).
    #[test]
    fn a_mobile_friend_is_followed_by_the_rest_of_the_selection() -> TestResult {
        let mut world = flat_world()?;
        let unit = soldier(&mut world, 0)?;
        let friend = soldier(&mut world, 1)?;
        let target = PointerTarget::Friendly(friend, POINT);
        assert_eq!(
            plan_on(&world, &[unit], target),
            [(follow(friend), vec![unit])]
        );
        assert_eq!(
            plan_on(&world, &[unit, friend], target),
            [(follow(friend), vec![unit])]
        );
        assert_eq!(plan_on(&world, &[friend], target), []);
        Ok(())
    }

    #[test]
    fn a_carrier_alone_does_not_board_itself() -> TestResult {
        let mut world = flat_world()?;
        let carrier = ground_carrier(&mut world, 0)?;
        assert_eq!(
            plan_on(&world, &[carrier], PointerTarget::Friendly(carrier, POINT)),
            []
        );
        Ok(())
    }

    /// P-1: at a dropoff the worker holding cargo returns it, and the worker without cargo and
    /// the soldier share one move to the ground.
    #[test]
    fn a_dropoff_takes_cargo_and_is_otherwise_ground() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let empty = worker(&mut world, 1)?;
        let other = soldier(&mut world, 2)?;
        let depot = structure_where(&mut world, unit, 20, |a| a.is_resource_dropoff)?;
        let target = PointerTarget::Friendly(depot, POINT);

        assert_eq!(
            plan_on(&world, &[unit], target),
            [(ground_move(), vec![unit])]
        );
        load(&mut world, unit)?;
        assert_eq!(
            plan_on(&world, &[unit, empty, other], target),
            [
                (UnitOrder::ReturnCargo, vec![unit]),
                (ground_move(), vec![empty, other]),
            ]
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
            plan_on(&world, &[unit, other], target),
            [
                (UnitOrder::Harvest { target: gas }, vec![unit]),
                (ground_move(), vec![other]),
            ]
        );
        assert_eq!(
            plan_on(&world, &[other], target),
            [(ground_move(), vec![other])]
        );
        unfinish(&mut world, extractor)?;
        assert_eq!(
            plan_on(&world, &[unit], target),
            [(
                UnitOrder::Construct {
                    foundation: extractor
                },
                vec![unit]
            )],
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

    /// P-1: the harvester harvests the node and the soldier walks to the ground there, which
    /// it was not given before (B-222).
    #[test]
    fn a_node_is_harvested_by_a_harvester_and_walked_to_by_the_rest() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        let mineral = ResourceNodeId(1);
        let target = PointerTarget::Node(mineral, POINT);

        assert_eq!(
            plan_on(&world, &[other, unit], target),
            [
                (UnitOrder::Harvest { target: mineral }, vec![unit]),
                (ground_move(), vec![other]),
            ]
        );
        assert_eq!(
            plan_on(&world, &[other], target),
            [(ground_move(), vec![other])]
        );
        Ok(())
    }

    /// Open ground means one move for every mobile member, and a structure in the selection
    /// takes none (it takes the rally, `issue_pointer_order`); an idle mobile friend, which
    /// offers nothing else, is followed (plan `2026-11-26-follow-order` D-10).
    #[test]
    fn open_ground_means_move_and_an_idle_friend_is_followed() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        let base = structure_where(&mut world, unit, 20, |a| a.is_resource_dropoff)?;
        assert_eq!(
            plan_on(&world, &[unit, base, other], PointerTarget::Ground(POINT)),
            [(ground_move(), vec![unit, other])]
        );
        assert_eq!(
            plan_on(&world, &[unit], PointerTarget::Friendly(other, POINT)),
            [(follow(other), vec![unit])]
        );
        Ok(())
    }

    #[test]
    fn a_unit_of_another_player_cannot_make_a_row_match() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        world.entities.get_mut(&unit).ok_or("absent")?.owner = THEM;
        assert_eq!(
            plan_on(
                &world,
                &[unit],
                PointerTarget::Node(ResourceNodeId(1), POINT)
            ),
            [],
            "a foreign member takes no row, not even the move"
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
            plan_on(&world, &[unit], PointerTarget::Friendly(carrier, POINT)),
            [(UnitOrder::Load { carrier }, vec![unit])]
        );
        Ok(())
    }

    /// P-1, agreement with today: a selection whose members all take the same row gives a
    /// one-entry plan with every member, the order the single-order rule gave.
    #[test]
    fn a_selection_that_agrees_gets_one_entry() -> TestResult {
        let mut world = flat_world()?;
        let first = worker(&mut world, 0)?;
        let second = worker(&mut world, 1)?;
        let mineral = ResourceNodeId(1);
        let both = [first, second];
        assert_eq!(
            plan_on(&world, &both, PointerTarget::Node(mineral, POINT)),
            [(UnitOrder::Harvest { target: mineral }, both.to_vec())]
        );
        assert_eq!(
            plan_on(&world, &both, PointerTarget::Ground(POINT)),
            [(ground_move(), both.to_vec())]
        );
        let enemy = enemy_where(&mut world, |a| {
            !is_dome(a) && a.mover_type == MoverType::Ground
        })?;
        let target = world
            .catalog
            .get(world.entities[&enemy].unit_type_id)?
            .clone();
        let one = unit_where(&mut world, 2, |a| hits(a, &target))?;
        let two = unit_where(&mut world, 3, |a| hits(a, &target))?;
        assert_eq!(
            plan_on(&world, &[one, two], PointerTarget::Enemy(enemy, POINT)),
            [(UnitOrder::attack(enemy), vec![one, two])]
        );
        Ok(())
    }

    /// D-3's no partial queue: a plan with an entry the queue refuses (a recipient that died
    /// after the plan was built) queues nothing, and a valid plan queues one command per entry
    /// in plan order, every one with the click's `queue`.
    #[test]
    fn a_plan_is_queued_whole_or_not_at_all() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world, 0)?;
        let other = soldier(&mut world, 1)?;
        let mineral = ResourceNodeId(1);
        let plan = smart_plan(
            &world,
            ME,
            &[unit, other],
            PointerTarget::Node(mineral, POINT),
        );
        let mut queue = BattleCommandQueue::default();
        queue.activate(SessionGeneration::FIRST)?;

        queue.submit_smart(&world, ME, &plan, true)?;
        let queued: Vec<_> = queue
            .pending_commands()
            .map(|(_, command)| {
                (
                    command.order.clone(),
                    command.entities.clone(),
                    command.queue,
                )
            })
            .collect();
        assert_eq!(
            queued,
            [
                (UnitOrder::Harvest { target: mineral }, vec![unit], true),
                (ground_move(), vec![other], true),
            ]
        );

        let mut fresh = BattleCommandQueue::default();
        fresh.activate(SessionGeneration::FIRST)?;
        world.entities.get_mut(&other).ok_or("absent")?.hp = SimScalar::ZERO;
        assert_eq!(
            fresh.submit_smart(&world, ME, &plan, false),
            Err(SubmissionError::UnavailableRecipient(other))
        );
        assert_eq!(fresh.pending_len(), 0, "nothing of the plan is queued");
        let empty = SmartPlan {
            orders: Vec::new(),
            marker_at: POINT,
        };
        assert_eq!(
            fresh.submit_smart(&world, ME, &empty, false),
            Err(SubmissionError::EmptySelection)
        );
        Ok(())
    }

    /// Plans `2026-11-23-production-queue` D-10 and `2026-11-25-rallies` D-8: each pointer
    /// target's rally. An enemy is the unit itself (B-166), and the player's own completed
    /// extractor is the node under it (B-216), while an unfinished one and a mere friend stay
    /// units.
    #[test]
    fn a_click_sets_the_rally_its_target_means() -> TestResult {
        let mut world = flat_world()?;
        let builder = worker(&mut world, 0)?;
        let extractor = structure_where(&mut world, builder, EXTRACTOR_CORNER, |a| {
            a.capabilities.is_gas_extractor
        })?;
        let friend = soldier(&mut world, 1)?;
        let ground = Vec3::new(12.0, 0.5, 34.0);
        let rally = |world: &SimWorld, target| rally_target(world, ME, target);
        assert_eq!(
            rally(&world, PointerTarget::Ground(ground)),
            RallyTarget::Point(sim_point(ground))
        );
        assert_eq!(
            rally(&world, PointerTarget::Enemy(EntityId(4), ground)),
            RallyTarget::Unit(EntityId(4)),
            "an enemy is the unit"
        );
        assert_eq!(
            rally(&world, PointerTarget::Node(ResourceNodeId(2), ground)),
            RallyTarget::Node(ResourceNodeId(2))
        );
        assert_eq!(
            rally(&world, PointerTarget::Friendly(friend, ground)),
            RallyTarget::Unit(friend)
        );
        assert_eq!(
            rally(&world, PointerTarget::Friendly(extractor, ground)),
            RallyTarget::Node(ResourceNodeId(0)),
            "the own extractor is its node"
        );
        assert_eq!(
            rally_target(&world, THEM, PointerTarget::Friendly(extractor, ground)),
            RallyTarget::Unit(extractor),
            "another player's extractor stays a unit"
        );
        unfinish(&mut world, extractor)?;
        assert_eq!(
            rally(&world, PointerTarget::Friendly(extractor, ground)),
            RallyTarget::Unit(extractor),
            "an unfinished extractor stays a unit"
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
