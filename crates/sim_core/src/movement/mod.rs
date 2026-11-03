//! Authored movement layers, atomic order admission and destination placement.
//!
//! Ground and conservative cliff movers share terrain navigation; Air only uses
//! full-radius map bounds and other Air colliders. Policies are derived from the
//! current catalog, never duplicated in authoritative state. Outcomes and scratch
//! buffers are derived evidence: consumers must drain outcomes after each tick and
//! surface invariant diagnostics before advancing an unhealthy match.

pub(crate) mod collision;
mod flee;
pub(crate) mod geometry;
mod grid;
mod locomotion;
pub(crate) mod math;
pub(crate) mod tuning;
pub use math::MovementMathError;

use crate::{Entity, SimWorld, SimulationDiagnostic, SteeringObstacle, UnitCatalog};
use sim_map::GameMap;
use sim_types::{
    AttackOrigin, EntityId, MoverType, PerPlayer, PlayerId, SimScalar, SimVec2, Tick, UnitOrder,
};
use thiserror::Error;

/// Collision/navigation domain derived from the authored mover capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementLayer {
    /// Ground and specialized cliff variants whose traversal capability is unresolved.
    Ground,
    /// Flight inside the current map rectangle; ground obstacles do not apply.
    Air,
}

impl MovementLayer {
    /// Classifies a mover without changing weapon targetability or vision semantics.
    #[must_use]
    pub const fn from_mover(mover: MoverType) -> Self {
        match mover {
            MoverType::Air => Self::Air,
            MoverType::Ground | MoverType::CliffJumper | MoverType::CliffWalker => Self::Ground,
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Ground => 0,
            Self::Air => 1,
        }
    }
}

/// Corruption or arithmetic that cannot be represented, distinct from player error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MovementInvariant {
    /// A malformed carrier relation invalidates independently requested world operations.
    #[error(transparent)]
    Transport(crate::transport::TransportInvariant),
    /// Malformed buff/payment admission suppressed the operation before commit.
    #[error("entity {entity:?} admission: {source}")]
    Admission {
        /// Actor whose requested mutation was rejected.
        entity: EntityId,
        /// Exact typed rate/cost failure.
        source: crate::entity::EntityAdmissionError,
    },
    /// Checked movement operation failed before its authoritative commit.
    #[error("movement entity {entity:?}, {operation}: {source}")]
    Arithmetic {
        /// Actor whose current operation was suppressed.
        entity: EntityId,
        /// Stable phase or admission name.
        operation: &'static str,
        /// Typed numeric or input failure.
        source: MovementMathError,
    },
    /// Typed collision arithmetic failure for a particular actor pair.
    #[error("movement entity {entity:?}: {source}")]
    PushGeometry {
        /// First actor in deterministic pair order.
        entity: EntityId,
        /// Exact checked geometry error.
        source: crate::steering::PushGeometryError,
    },
    /// An existing actor's catalog definition cannot be resolved.
    #[error("movement entity {0:?} references a missing archetype")]
    MissingArchetype(EntityId),
    /// A mutable definition or actor state violates the movement contract.
    #[error("movement entity {entity:?}: {reason}")]
    InvalidState {
        /// Existing actor whose displacement is suppressed.
        entity: EntityId,
        /// Failed invariant, expressed in simulation units.
        reason: &'static str,
    },
}

/// Ordinary Move/AttackMove rejection or an explicitly diagnosed invariant failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MovementAdmissionError {
    /// Carried actors cannot accept world-space movement intent.
    #[error("movement recipient {0:?} is carried")]
    LoadedRecipient(EntityId),
    /// Unsupported player slot.
    #[error("invalid movement player {0:?}")]
    InvalidPlayer(PlayerId),
    /// No selected actors.
    #[error("select at least one movement recipient")]
    EmptySelection,
    /// Selected actor does not exist.
    #[error("movement recipient {0:?} does not exist")]
    MissingRecipient(EntityId),
    /// Selected actor is owned by another player.
    #[error("movement recipient {0:?} is not owned by this player")]
    ForeignRecipient(EntityId),
    /// Selected actor is destroyed.
    #[error("movement recipient {0:?} is destroyed")]
    DeadRecipient(EntityId),
    /// The destination cannot contain this actor's full radius.
    #[error("movement destination lies outside the full-radius map domain of {0:?}")]
    OutsideDomain(EntityId),
    /// This API only admits `Move` and `AttackMove` orders.
    #[error("movement admission requires Move or AttackMove")]
    UnsupportedOrder,
    /// Authoritative state is malformed; also emitted through diagnostics.
    #[error(transparent)]
    Invariant(#[from] MovementInvariant),
}

/// Accepted scheduling intent, not proof that recipients reached their destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovementReceipt {
    /// Number of distinct recipients whose orders were committed.
    pub recipient_count: usize,
}

/// One command-level result correlated to the original tick bundle command index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovementOutcome {
    /// Incremented execution tick, matching production outcome semantics.
    pub tick: Tick,
    /// Index in the exact `TickBundle.commands` processed in this tick.
    pub command_index: usize,
    /// Issuing player; retained on invalid-player rejection.
    pub player: PlayerId,
    /// Atomic selection result. Normal rejection is nonfatal.
    pub result: Result<MovementReceipt, MovementAdmissionError>,
}

/// What became of a unit's route, for callers that must show or react to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteOutcomeKind {
    /// The destination cannot be reached, so the order now ends at the nearest point that can.
    Retargeted {
        /// Destination in the order as issued, in world units.
        requested: SimVec2,
        /// Nearest reachable point the unit walks to instead, in world units.
        reachable: SimVec2,
    },
    /// No point closer to the destination is reachable from here; the order ended.
    Unreachable,
    /// The unit stopped getting closer for too long; the order ended where it stands.
    NoProgress,
    /// An attack order ended because its target has no route the unit may take.
    PursuitAbandoned {
        /// The target given up on.
        target: EntityId,
    },
}

/// One unit's route event during a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteOutcome {
    /// Executed tick that produced this event.
    pub tick: Tick,
    /// Unit concerned.
    pub entity: EntityId,
    /// What happened.
    pub kind: RouteOutcomeKind,
}

/// Exact destination placement rejection; no nearest-free substitution is performed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DisplacementAdmissionError {
    /// Foundation relocation requires an atomic map-placement workflow.
    #[error("registered foundation {0:?} cannot be teleported or unloaded")]
    RegisteredFoundation(EntityId),
    /// Requested actor is absent.
    #[error("placement actor {0:?} does not exist")]
    MissingEntity(EntityId),
    /// A destroyed actor cannot be placed.
    #[error("placement actor {0:?} is destroyed")]
    DeadEntity(EntityId),
    /// Full radius crosses a map boundary.
    #[error("placement for {0:?} crosses map bounds")]
    OutsideDomain(EntityId),
    /// Ground footprint contains disconnected terrain or occupied structure cells.
    #[error("placement for {0:?} has a blocked ground footprint")]
    Terrain(EntityId),
    /// A living actor in the same collision layer occupies the requested circle.
    #[error("placement is occupied by {0:?}")]
    Occupied(EntityId),
    /// Invalid live definitions; callers must report this as corruption.
    #[error(transparent)]
    Invariant(#[from] MovementInvariant),
}

#[derive(Default)]
pub(crate) struct MovementWorkspace {
    pub outcomes: Vec<MovementOutcome>,
    pub route_outcomes: Vec<RouteOutcome>,
    pub path_searches: u64,
    pub obstacles: [Vec<SteeringObstacle>; 2],
    grids: [grid::BodyGrid; 2],
    largest_radius: [SimScalar; 2],
    neighbours: Vec<SteeringObstacle>,
    recipients: Vec<EntityId>,
    entries: Vec<locomotion::MotionEntry>,
    /// Whether `entries` came from this tick's movement phase and may serve collision.
    entries_are_fresh: bool,
    bodies: Vec<collision::CollisionBody>,
    pushes: Vec<collision::BodyPush>,
    collision_grid: grid::BodyGrid,
    pair_scratch: Vec<usize>,
    phase_ids: Vec<EntityId>,
    pursuits: Vec<(locomotion::MotionEntry, EntityId, AttackOrigin)>,
    casts: Vec<(locomotion::MotionEntry, locomotion::CastRequest)>,
    reach_cache: Option<locomotion::ReachCache>,
    /// Per player: knowledge of the map changed, so every route is looked at this tick.
    pub recheck_routes: PerPlayer<bool>,
    seen_nav_version: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct MotionPolicy {
    pub layer: MovementLayer,
    pub radius: SimScalar,
}

impl MotionPolicy {
    pub fn resolve(
        catalog: &UnitCatalog,
        map: &GameMap,
        entity: &Entity,
    ) -> Result<Self, MovementInvariant> {
        let arch = catalog
            .get(entity.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(entity.id))?;
        let radius = arch.collision_radius;
        if radius <= SimScalar::ZERO
            || i128::from(radius.raw()) * 2 > (i128::from(map.width().min(map.height())) << 32)
        {
            return Err(MovementInvariant::InvalidState {
                entity: entity.id,
                reason: "collision radius must be positive and fit inside the map",
            });
        }
        Ok(Self {
            layer: MovementLayer::from_mover(arch.mover_type),
            radius,
        })
    }

    pub fn validate_origin(self, map: &GameMap, actor: &Entity) -> Result<(), MovementInvariant> {
        if !self.contains(map, actor.position) {
            return Err(MovementInvariant::InvalidState {
                entity: actor.id,
                reason: "actor is outside its current full-radius movement domain",
            });
        }
        if self.layer == MovementLayer::Ground
            && !SimWorld::is_position_pathable(map, actor.position, actor.position)
        {
            return Err(MovementInvariant::InvalidState {
                entity: actor.id,
                reason: "Ground actor's current center is not traversable terrain",
            });
        }
        Ok(())
    }

    pub fn contains(self, map: &GameMap, point: SimVec2) -> bool {
        let r = i128::from(self.radius.raw());
        let x = i128::from(point.x.raw());
        let y = i128::from(point.y.raw());
        x - r >= 0
            && y - r >= 0
            && x + r <= (i128::from(map.width()) << 32)
            && y + r <= (i128::from(map.height()) << 32)
    }

    pub fn permits_segment(self, map: &GameMap, start: SimVec2, end: SimVec2) -> bool {
        if !self.contains(map, end) {
            return false;
        }
        match self.layer {
            MovementLayer::Air => self.contains(map, start),
            MovementLayer::Ground => SimWorld::is_position_pathable(map, start, end),
        }
    }
}

/// Exact circle overlap without overflow even for extreme signed setup coordinates.
pub(crate) fn circles_overlap(a: SimVec2, ar: SimScalar, b: SimVec2, br: SimScalar) -> bool {
    let dx = (i128::from(a.x.raw()) - i128::from(b.x.raw())).unsigned_abs();
    let dy = (i128::from(a.y.raw()) - i128::from(b.y.raw())).unsigned_abs();
    let sum = (i128::from(ar.raw()) + i128::from(br.raw())).unsigned_abs();
    dx < sum && dy < sum && dx * dx < sum * sum - dy * dy
}

impl SimWorld {
    /// Returns command-level movement results since the latest tick clear/drain.
    #[must_use]
    pub fn movement_outcomes(&self) -> &[MovementOutcome] {
        &self.movement.outcomes
    }

    /// Drains outcomes without discarding capacity. Consume after every tick before
    /// catch-up advances again; indices belong to that exact tick's input bundle.
    pub fn drain_movement_outcomes(&mut self) -> std::vec::Drain<'_, MovementOutcome> {
        self.movement.outcomes.drain(..)
    }

    /// Route events of the latest tick. Derived output, excluded from checksums and never
    /// consulted by the simulation; cleared at the start of every tick.
    #[must_use]
    pub fn route_outcomes(&self) -> &[RouteOutcome] {
        &self.movement.route_outcomes
    }

    /// Total pathfinder searches this world has run. Derived output for tests and benchmarks.
    #[must_use]
    pub fn path_search_count(&self) -> u64 {
        self.movement.path_searches
    }

    pub(crate) fn report_movement_invariant(&mut self, source: MovementInvariant) {
        self.tick_diagnostics
            .push(SimulationDiagnostic::MovementInvariant {
                tick: Tick(self.current_tick),
                source,
            });
    }

    /// Atomically admits one Move/AttackMove order for all unique selected actors.
    /// Queuing preserves existing orders; replacement preserves normal weapon interruption.
    /// Destination coordinates are world units, validated against each actual radius.
    ///
    /// # Errors
    /// Rejects the entire selection before mutation on invalid recipients or domain.
    /// Malformed live definitions additionally emit a fatal simulation diagnostic.
    pub fn try_issue_movement(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        order: &UnitOrder,
        queue: bool,
    ) -> Result<MovementReceipt, MovementAdmissionError> {
        if let Err(source) = self.validate_transport_links() {
            let error = MovementInvariant::Transport(source);
            self.report_movement_invariant(error.clone());
            return Err(error.into());
        }
        self.issue_movement_prevalidated(player, recipients, order, queue)
    }

    // Called after a public wrapper/tick validates the graph. Movement changes no links.
    pub(crate) fn issue_movement_prevalidated(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        order: &UnitOrder,
        queue: bool,
    ) -> Result<MovementReceipt, MovementAdmissionError> {
        let prepared = self.validate_movement_order(player, recipients, order);
        if let Err(MovementAdmissionError::Invariant(ref error)) = prepared {
            self.report_movement_invariant(error.clone());
        }
        prepared?;
        // PERFORMANCE RATIONALE:
        // Why: Repeated prefix searches make large selections quadratic.
        // What: Retain one sorted/deduplicated ID buffer for O(N log N) commits.
        // Trade-off: Capacity grows at selection high-water marks; validation still
        // follows input order so the first rejection remains stable and meaningful.
        self.movement.recipients.clear();
        self.movement.recipients.extend_from_slice(recipients);
        self.movement.recipients.sort_unstable();
        self.movement.recipients.dedup();
        let receipt = MovementReceipt {
            recipient_count: self.movement.recipients.len(),
        };
        for id in &self.movement.recipients {
            // Exclusive preparation verified every identity; no intervening world mutation.
            if let Some(entity) = self.entities.get_mut(id) {
                if let UnitOrder::Move { target } | UnitOrder::AttackMove { target } = order {
                    entity.commit_movement_order(
                        *target,
                        matches!(order, UnitOrder::AttackMove { .. }),
                        queue,
                    );
                }
            }
        }
        Ok(receipt)
    }

    fn validate_movement_order(
        &self,
        player: PlayerId,
        recipients: &[EntityId],
        order: &UnitOrder,
    ) -> Result<(), MovementAdmissionError> {
        if player.slot().is_none() {
            return Err(MovementAdmissionError::InvalidPlayer(player));
        }
        let target = match order {
            UnitOrder::Move { target } | UnitOrder::AttackMove { target } => *target,
            _ => return Err(MovementAdmissionError::UnsupportedOrder),
        };
        if recipients.is_empty() {
            return Err(MovementAdmissionError::EmptySelection);
        }
        for id in recipients {
            let entity = self
                .entities
                .get(id)
                .ok_or(MovementAdmissionError::MissingRecipient(*id))?;
            if entity.owner != player {
                return Err(MovementAdmissionError::ForeignRecipient(*id));
            }
            if !entity.is_alive() {
                return Err(MovementAdmissionError::DeadRecipient(*id));
            }
            if entity.is_loaded_in_transport() {
                return Err(MovementAdmissionError::LoadedRecipient(*id));
            }
            let policy = MotionPolicy::resolve(&self.catalog, &self.map, entity)?;
            if !policy.contains(&self.map, target) {
                return Err(MovementAdmissionError::OutsideDomain(*id));
            }
        }
        Ok(())
    }

    /// Validates an exact teleport/unload endpoint, without mutating any actor fields.
    /// Ground uses conservative square terrain clearance; live occupancy uses circles
    /// within the same layer. Intervening terrain is irrelevant for teleport placement.
    ///
    /// # Errors
    /// Returns a typed destination rejection or malformed live-state invariant.
    pub fn validate_displacement_destination(
        &self,
        id: EntityId,
        point: SimVec2,
    ) -> Result<(), DisplacementAdmissionError> {
        self.validate_transport_links()
            .map_err(MovementInvariant::Transport)?;
        self.validate_displacement_destination_prevalidated(id, point)
    }

    // Blink and unload already validated ownership; placement does not change links.
    pub(crate) fn validate_displacement_destination_prevalidated(
        &self,
        id: EntityId,
        point: SimVec2,
    ) -> Result<(), DisplacementAdmissionError> {
        let actor = self
            .entities
            .get(&id)
            .ok_or(DisplacementAdmissionError::MissingEntity(id))?;
        if !actor.is_alive() {
            return Err(DisplacementAdmissionError::DeadEntity(id));
        }
        if self.map.structures().contains_key(&id) {
            return Err(DisplacementAdmissionError::RegisteredFoundation(id));
        }
        let policy = MotionPolicy::resolve(&self.catalog, &self.map, actor)?;
        if !policy.contains(&self.map, point) {
            return Err(DisplacementAdmissionError::OutsideDomain(id));
        }
        if policy.layer == MovementLayer::Ground
            && !geometry::ground_footprint_connected(&self.map, point, point, policy.radius, None)
        {
            return Err(DisplacementAdmissionError::Terrain(id));
        }
        for other in self
            .entities
            .values()
            .filter(|e| e.id != id && e.is_alive() && !e.is_loaded_in_transport())
        {
            let other_policy = MotionPolicy::resolve(&self.catalog, &self.map, other)?;
            if policy.layer == other_policy.layer
                && circles_overlap(point, policy.radius, other.position, other_policy.radius)
            {
                return Err(DisplacementAdmissionError::Occupied(other.id));
            }
        }
        Ok(())
    }
}

/// Euclidean distance in raw Q32.32, widening before squaring. The squared
/// intermediate is Q64.64 and never narrows to `SimScalar`. If its u128 sum or
/// final i64 root does not fit, the final distance itself cannot be represented.
#[must_use]
pub fn checked_distance(a: SimVec2, b: SimVec2) -> Option<SimScalar> {
    let x = (i128::from(a.x.raw()) - i128::from(b.x.raw())).unsigned_abs();
    let y = (i128::from(a.y.raw()) - i128::from(b.y.raw())).unsigned_abs();
    let squared = x.checked_mul(x)?.checked_add(y.checked_mul(y)?)?;
    let root = integer_sqrt(squared);
    i64::try_from(root).ok().map(SimScalar::from_raw)
}

fn integer_sqrt(value: u128) -> u128 {
    if value == 0 {
        return 0;
    }
    // Initial upper bound has at most 65 bits. Newton iterates decrease, so
    // x + value/x remains below 2^66 and division never uses zero.
    let mut x = 1_u128 << (128 - value.leading_zeros()).div_ceil(2);
    loop {
        let next = u128::midpoint(x, value / x);
        if next >= x {
            return x;
        }
        x = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_movement_workspaces_keep_capacity_after_population_warmup(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 45);
        let ground = world
            .catalog
            .archetypes()
            .find(|a| a.mover_type == MoverType::Ground && !a.is_immobile_by_default())
            .map(|a| a.unit_type_id)
            .ok_or("Ground actor")?;
        let air = world
            .catalog
            .archetypes()
            .find(|a| a.mover_type == MoverType::Air && !a.is_immobile_by_default())
            .map(|a| a.unit_type_id)
            .ok_or("Air actor")?;
        let mut ids = Vec::new();
        for index in 0..32 {
            ids.push(world.spawn_unit(
                if index % 2 == 0 { ground } else { air },
                PlayerId(0),
                SimVec2::new(
                    SimScalar::from_num(8 + index % 4),
                    SimScalar::from_num(8 + index / 4),
                ),
            )?);
        }
        let order = UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(24), SimScalar::from_num(24)),
        };
        world.try_issue_movement(PlayerId(0), &ids, &order, false)?;
        world.step_movement_and_steering();
        let capacities = (
            world.movement.obstacles[0].capacity(),
            world.movement.obstacles[1].capacity(),
            world.movement.recipients.capacity(),
        );
        for _ in 0..8 {
            world.try_issue_movement(PlayerId(0), &ids, &order, false)?;
            world.step_movement_and_steering();
            assert_eq!(
                (
                    world.movement.obstacles[0].capacity(),
                    world.movement.obstacles[1].capacity(),
                    world.movement.recipients.capacity()
                ),
                capacities
            );
        }
        assert_eq!(world.diagnostics(), []);
        Ok(())
    }

    #[test]
    fn wide_distance_keeps_representable_results_and_rejects_only_final_overflow() {
        let origin = SimVec2::ZERO;
        let point = SimVec2::new(SimScalar::from_num(30_000), SimScalar::from_num(40_000));
        assert_eq!(
            checked_distance(origin, point),
            Some(SimScalar::from_num(50_000))
        );
        let max = SimVec2::new(SimScalar::from_raw(i64::MAX), SimScalar::ZERO);
        assert_eq!(
            checked_distance(origin, max),
            Some(SimScalar::from_raw(i64::MAX))
        );
        let min = SimVec2::new(SimScalar::from_raw(i64::MIN), SimScalar::ZERO);
        assert_eq!(checked_distance(min, max), None);
        assert_eq!(integer_sqrt(u128::MAX), u128::from(u64::MAX));
    }
}
