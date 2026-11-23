//! Authored movement layers, atomic order admission and destination placement.
//!
//! Ground and conservative cliff movers share terrain navigation; Air only uses
//! map bounds and other Air colliders. A body meets terrain, structures and the map edge at its
//! terrain radius (`StarCraft` II `min(InnerRadius, Radius)`) and other bodies at its collision
//! radius; for Air the two are equal. Policies are derived from the
//! current catalog, never duplicated in authoritative state. Outcomes and scratch
//! buffers are derived evidence: consumers must drain outcomes after each tick and
//! surface invariant diagnostics before advancing an unhealthy match.

pub(crate) mod collision;
pub(crate) mod concave;
pub(crate) mod contact;
mod flee;
pub(crate) mod geometry;
mod grid;
pub mod groups;
pub(crate) mod intent;
mod locomotion;
pub(crate) mod math;
mod ring;
pub mod solver;
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

/// Number of body classes ([`MotionPolicy::body_class`]): ground, air and burrowed.
pub(crate) const BODY_CLASSES: usize = 3;

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
    /// The destination cannot contain this actor's terrain radius
    /// ([`crate::catalog::UnitArchetype::terrain_radius`]) inside the map.
    #[error("movement destination lies outside the terrain-radius map domain of {0:?}")]
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
    /// A route group member ran the restricted search for its group's lane (plan
    /// `2026-11-10-group-routing`, D-4 step 5): its own route was in another lane.
    GroupLane {
        /// The member's group.
        group: groups::RouteGroupId,
        /// Length of the member's own shortest route, in world units.
        own_length: SimScalar,
        /// Length of the shortest route in the group's lane, in world units; `None` when the
        /// lane admits no route for the member.
        lane_length: Option<SimScalar>,
        /// Whether the member walks the lane route (it is within the tolerance).
        adopted: bool,
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
    /// The actor's terrain radius crosses a map boundary.
    #[error("placement for {0:?} crosses map bounds")]
    OutsideDomain(EntityId),
    /// Ground footprint contains disconnected terrain or occupied structure cells.
    #[error("placement for {0:?} has a blocked ground footprint")]
    Terrain(EntityId),
    /// A living unit in the same collision layer occupies the requested circle. Never a
    /// registered structure: structures refuse a point through their footprint, as `Terrain`
    /// (plan `2026-11-18-structure-contact`, B-131).
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
    pub obstacles: [Vec<SteeringObstacle>; BODY_CLASSES],
    grids: [grid::BodyGrid; BODY_CLASSES],
    largest_radius: [SimScalar; BODY_CLASSES],
    neighbours: Vec<SteeringObstacle>,
    recipients: Vec<EntityId>,
    entries: Vec<locomotion::MotionEntry>,
    /// Whether `entries` came from this tick's movement phase and may serve collision.
    entries_are_fresh: bool,
    bodies: Vec<collision::CollisionBody>,
    pushes: Vec<collision::BodyPush>,
    collision_grid: grid::BodyGrid,
    pair_scratch: Vec<usize>,
    /// The contact solver's retained buffers (plan `2026-11-20-contact-solver` C-1).
    pub(crate) solver: solver::ContactSolver,
    /// Averaged iterations the contact solver ran in the latest collision phase; 0 under the legacy
    /// pipeline. Derived output for the cost report: never read by the simulation, never hashed.
    contact_iterations: u8,
    /// Pair-list rebuilds of the contact solver in the latest collision phase (Verlet rule).
    /// Derived output for the cost report, like [`Self::contact_iterations`].
    contact_rebuilds: u8,
    phase_ids: Vec<EntityId>,
    pursuits: Vec<(locomotion::MotionEntry, EntityId, AttackOrigin)>,
    /// Each movement actor's position and last displacement when the phase began, sorted by
    /// entity id (plan `2026-11-20-contact-solver` Amendment 7 and WP5: the ring rule and, under
    /// the solver pipeline, the pursuit read it).
    tick_start: Vec<(EntityId, SimVec2, SimVec2)>,
    /// The ring rule's blocked arcs, cleared per chase (Amendment 7, item 2).
    ring_intervals: Vec<concave::ArcInterval>,
    casts: Vec<(locomotion::MotionEntry, locomotion::CastRequest)>,
    reach_cache: Option<locomotion::ReachCache>,
    /// Each actor's committed proposal velocity of the latest movement phase, by entity id once
    /// the phase ends (plan `2026-11-19-crowd-edge-steering` D-5). Derived output for the
    /// reversal meter: never read by the simulation, never hashed, snapshotted or replayed.
    proposed_steps: Vec<(EntityId, SimVec2)>,
    /// Per player: knowledge of the map changed, so every route is looked at this tick.
    pub recheck_routes: PerPlayer<bool>,
    seen_nav_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MotionPolicy {
    pub layer: MovementLayer,
    /// The body's size against other bodies (`collision_radius`), in world units.
    pub radius: SimScalar,
    /// The body's disc against terrain, structures and the map edge (`terrain_radius`), in world
    /// units; `0 < terrain_radius <= radius`. Read through [`Self::terrain_radius`].
    pub terrain: SimScalar,
    /// The body is burrowed ([`Entity::is_burrowed`]): it touches only burrowed bodies.
    pub burrowed: bool,
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
        let terrain = arch.terrain_radius;
        if terrain <= SimScalar::ZERO || terrain > radius {
            return Err(MovementInvariant::InvalidState {
                entity: entity.id,
                reason: "terrain radius must be positive and at most the collision radius",
            });
        }
        let layer = MovementLayer::from_mover(arch.mover_type);
        // Terrain does not touch an air body, and its domain (`contains`) reads the terrain
        // radius, so an air body's must be its collision radius (plan D-2).
        if layer == MovementLayer::Air && terrain != radius {
            return Err(MovementInvariant::InvalidState {
                entity: entity.id,
                reason: "an air body's terrain radius must equal its collision radius",
            });
        }
        Ok(Self {
            layer,
            radius,
            terrain,
            burrowed: entity.is_burrowed(arch),
        })
    }

    /// The radius of the body's disc against terrain, structures and the map edge, in world
    /// units (plan `2026-11-11-contact-and-missiles` D-1, plan `2026-11-18-inner-radius` D-5):
    /// the archetype's `terrain_radius`, `StarCraft` II's `min(InnerRadius, Radius)`. Bodies
    /// meet each other at [`Self::radius`].
    pub const fn terrain_radius(self) -> SimScalar {
        self.terrain
    }

    /// Which bodies this one touches: two bodies steer around, push and block each other only
    /// when their classes are equal. Ground and air bodies keep their layer's index; a burrowed
    /// body has a class of its own, below the ground, so ground units walk over it and it
    /// neither pushes nor is pushed by them (plan `2026-11-09-friendly-blocking` R6, B-93).
    /// Terrain still follows [`Self::layer`].
    pub const fn body_class(self) -> usize {
        if self.burrowed {
            2
        } else {
            self.layer.index()
        }
    }

    pub fn validate_origin(self, map: &GameMap, actor: &Entity) -> Result<(), MovementInvariant> {
        if !self.contains(map, actor.position) {
            return Err(MovementInvariant::InvalidState {
                entity: actor.id,
                reason: "actor is outside its current terrain-radius movement domain",
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

    /// Whether the body's terrain disc at `point` lies inside the map: the map edge is a wall,
    /// so the domain uses the same radius as the routes and the contact rule (plan
    /// `2026-11-18-inner-radius` D-7).
    pub fn contains(self, map: &GameMap, point: SimVec2) -> bool {
        let r = i128::from(self.terrain_radius().raw());
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

    /// The step `id` proposed in the latest movement phase, world units per tick: the velocity of
    /// its committed motion proposal, before collision corrected it (plan
    /// `2026-11-19-crowd-edge-steering` D-5). `None` when `id` took no part in that phase, or
    /// its action was a cast rather than a motion. Derived output, excluded from checksums and
    /// never consulted by the simulation, like [`Self::route_outcomes`].
    #[must_use]
    pub fn proposed_step(&self, id: EntityId) -> Option<SimVec2> {
        let steps = &self.movement.proposed_steps;
        steps
            .binary_search_by_key(&id, |&(entity, _)| entity)
            .ok()
            .map(|index| steps[index].1)
    }

    /// Averaged iterations the contact solver ran in the latest collision phase (plan
    /// `2026-11-20-contact-solver` C-1, the early stop); 0 under the legacy pipeline. Derived
    /// output for the cost report, excluded from checksums and never consulted by the simulation.
    #[must_use]
    pub fn contact_iterations(&self) -> u8 {
        self.movement.contact_iterations
    }

    /// Pair-list rebuilds of the contact solver in the latest collision phase (plan
    /// `2026-11-20-contact-solver` Amendment 1, the Verlet rule); 0 under the legacy pipeline.
    /// Derived output for the cost report, excluded from checksums.
    #[must_use]
    pub fn contact_rebuilds(&self) -> u8 {
        self.movement.contact_rebuilds
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
        let recipients = std::mem::take(&mut self.movement.recipients);
        self.form_route_group(player, &recipients, order);
        self.movement.recipients = recipients;
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
        self.validate_placement_terrain(id, point)?;
        self.validate_placement_unoccupied(id, point)
    }

    /// Validates `point` for `id`'s movement layer against terrain and structure footprints
    /// only, ignoring every other unit: the placement of an effect that sets
    /// `PlacementIgnoreBlockers` (`StarCraft` II `CEffectCreateUnit`; Abduct's landing point,
    /// plan `2026-11-10-spellcaster-mechanics` D-8). Structures block through the map's
    /// footprint cells; the overlap of units is resolved afterwards by separation.
    ///
    /// # Errors
    /// Returns a typed destination rejection or malformed live-state invariant.
    pub(crate) fn validate_placement_terrain(
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
            && !geometry::ground_footprint_connected(
                &self.map,
                point,
                point,
                policy.terrain_radius(),
                None,
            )
        {
            return Err(DisplacementAdmissionError::Terrain(id));
        }
        Ok(())
    }

    /// Validates that no other live unit of `id`'s body class overlaps `point`, each at its
    /// collision radius. A registered structure is not a body (plan
    /// `2026-11-18-structure-contact` D-2 B3, B-131): its footprint has already judged `point` at
    /// the terrain radius in [`Self::validate_placement_terrain`], so it is skipped here.
    fn validate_placement_unoccupied(
        &self,
        id: EntityId,
        point: SimVec2,
    ) -> Result<(), DisplacementAdmissionError> {
        let actor = self
            .entities
            .get(&id)
            .ok_or(DisplacementAdmissionError::MissingEntity(id))?;
        let policy = MotionPolicy::resolve(&self.catalog, &self.map, actor)?;
        for other in self.entities.values().filter(|e| {
            e.id != id
                && e.is_alive()
                && !e.is_loaded_in_transport()
                && !self.map.structures().contains_key(&e.id)
        }) {
            let other_policy = MotionPolicy::resolve(&self.catalog, &self.map, other)?;
            if policy.body_class() == other_policy.body_class()
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

/// The floor of the square root of `value`: the largest `r` with `r² ≤ value`.
// PERFORMANCE RATIONALE:
// Why: Every contact pair, steering normal and distance takes one root; a Newton loop on u128
// spends five to seven 128-bit software divisions per root (plan `2026-11-20-contact-solver`
// Amendment 3, item 1).
// What: Below 2^64 the 64-bit integer root. Above, the root of the top 64 bits gives the high
// bits exactly (`⌊√⌊v / 4^k⌋⌋ = ⌊√v / 2^k⌋`), one 64-bit division of the remainder by twice
// that root gives the low bits to within one, and squaring corrects the last unit. The floor
// root is unique, so the result equals the Newton loop's bit for bit (tested against it).
// Trade-off: Three paths instead of one loop, for about half the time and no more instructions.
pub(crate) fn integer_sqrt(value: u128) -> u128 {
    if let Ok(small) = u64::try_from(value) {
        return u128::from(small.isqrt());
    }
    let bits = 128 - value.leading_zeros();
    let shift = (bits - 64).div_ceil(2);
    let top = u64::try_from(value >> (2 * shift)).unwrap_or(u64::MAX);
    let high = u128::from(top.isqrt()) << shift;
    // high ≤ ⌊√value⌋ < 2^64, so its square does not exceed `value` and fits.
    let rest = value - high * high;
    let mut root = match (u64::try_from(rest), u64::try_from(2 * high)) {
        (Ok(rest), Ok(twice)) => high + u128::from(rest / twice),
        _ => low_bits_by_squares(value, high, shift),
    };
    while root.checked_mul(root).is_none_or(|square| square > value) {
        root -= 1;
    }
    while (root + 1)
        .checked_mul(root + 1)
        .is_some_and(|square| square <= value)
    {
        root += 1;
    }
    root
}

/// Completes the root `high` of `value`, exact in all but its `shift` low bits, one bit at a
/// time from the top: the wide fallback of [`integer_sqrt`].
fn low_bits_by_squares(value: u128, high: u128, shift: u32) -> u128 {
    let mut root = high;
    for bit in (0..shift).rev() {
        // The root of a u128 is below 2^64, so the candidate's square fits.
        let candidate = root | (1_u128 << bit);
        if candidate * candidate <= value {
            root = candidate;
        }
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review F-1 (plan `2026-11-18-inner-radius` D-2): an air archetype built past the loader
    /// with a terrain radius other than its collision radius is a movement invariant; its domain
    /// reads the terrain radius, so it must be the radius the air body has.
    #[test]
    fn an_air_body_with_a_smaller_terrain_radius_is_an_invariant(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 47);
        let kind = world
            .catalog
            .archetypes()
            .find(|a| a.mover_type == MoverType::Air && !a.is_immobile_by_default())
            .map(|a| a.unit_type_id)
            .ok_or("Air actor")?;
        let id = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(10, 10))?;
        let radius = world.catalog.get(kind)?.collision_radius;
        let resolve = |world: &SimWorld| {
            MotionPolicy::resolve(&world.catalog, &world.map, &world.entities[&id])
        };
        assert_eq!(resolve(&world)?.terrain_radius(), radius);
        world.catalog.get_mut(kind)?.terrain_radius = SimScalar::from_raw(radius.raw() / 2);
        assert!(matches!(
            resolve(&world),
            Err(MovementInvariant::InvalidState {
                reason: "an air body's terrain radius must equal its collision radius",
                ..
            })
        ));
        Ok(())
    }

    /// T-12 (plan `2026-11-18-inner-radius` D-2): an archetype built past the loader with a
    /// terrain radius above its collision radius, or not positive, is a movement invariant; one
    /// at or below it resolves to a policy that reports it.
    #[test]
    fn a_terrain_radius_above_the_collision_radius_is_an_invariant(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 46);
        let kind = world
            .catalog
            .archetypes()
            .find(|a| a.mover_type == MoverType::Ground && !a.is_immobile_by_default())
            .map(|a| a.unit_type_id)
            .ok_or("Ground actor")?;
        let id = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(10, 10))?;
        let radius = world.catalog.get(kind)?.collision_radius;
        let half = SimScalar::from_raw(radius.raw() / 2);
        for (terrain, valid) in [
            (radius, true),
            (half, true),
            (radius + SimScalar::from_raw(1), false),
            (SimScalar::ZERO, false),
            (-half, false),
        ] {
            world.catalog.get_mut(kind)?.terrain_radius = terrain;
            let resolved = MotionPolicy::resolve(&world.catalog, &world.map, &world.entities[&id]);
            if valid {
                let policy = resolved.map_err(|e| format!("{terrain:?}: {e}"))?;
                assert_eq!(policy.terrain_radius(), terrain);
                assert_eq!(policy.radius, radius);
            } else {
                assert!(
                    matches!(
                        resolved,
                        Err(MovementInvariant::InvalidState {
                            reason:
                                "terrain radius must be positive and at most the collision radius",
                            ..
                        })
                    ),
                    "{terrain:?}"
                );
            }
        }
        Ok(())
    }
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

    /// Plan `2026-11-19-crowd-edge-steering` D-5 and §8: `proposed_step` returns each actor's
    /// committed proposal of the latest movement phase, `None` for a body that took no part in
    /// it, and reading it changes nothing a checksum sees.
    #[test]
    fn the_proposed_step_is_the_committed_proposal_and_is_not_state(
    ) -> Result<(), Box<dyn std::error::Error>> {
        type Built = (SimWorld, [EntityId; 3]);
        let build = || -> Result<Built, Box<dyn std::error::Error>> {
            let map = GameMap::new(
                32,
                32,
                sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
            )?;
            let mut world = crate::test_support::world(map, 44);
            let kind = world
                .catalog
                .archetypes()
                .find(|a| {
                    a.mover_type == MoverType::Ground
                        && !a.is_immobile_by_default()
                        && a.max_speed > SimScalar::ZERO
                })
                .map(|a| a.unit_type_id)
                .ok_or("Ground actor")?;
            let walker = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(8, 8))?;
            let stander = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(20, 20))?;
            let dead = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(8, 20))?;
            world.entities.get_mut(&dead).ok_or("dead")?.hp = SimScalar::ZERO;
            let order = UnitOrder::Move {
                target: SimVec2::from_i32(24, 8),
            };
            world.try_issue_movement(PlayerId(0), &[walker], &order, false)?;
            world.step_movement_and_steering();
            Ok((world, [walker, stander, dead]))
        };
        let (world, [walker, stander, dead]) = build()?;
        let (twin, _) = build()?;
        assert_eq!(world.diagnostics(), []);
        // Before collision runs, the achieved velocity is the committed proposal's.
        let walked = world.entities[&walker].velocity;
        assert!(walked.x > SimScalar::ZERO, "{walked:?}");
        assert_eq!(world.proposed_step(walker), Some(walked));
        assert_eq!(world.proposed_step(stander), Some(SimVec2::ZERO));
        assert_eq!(world.proposed_step(dead), None);
        assert_eq!(world.proposed_step(EntityId(9_999)), None);
        // One world was read and its twin not: their state is the same.
        assert_eq!(world.compute_checksum(), twin.compute_checksum());
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

    /// The Newton loop the root replaced (plan `2026-11-20-contact-solver` Amendment 3, item 1):
    /// the oracle of [`integer_sqrt_equals_the_newton_root_bit_for_bit`].
    fn newton_sqrt(value: u128) -> u128 {
        if value == 0 {
            return 0;
        }
        let mut x = 1_u128 << (128 - value.leading_zeros()).div_ceil(2);
        loop {
            let next = u128::midpoint(x, value / x);
            if next >= x {
                return x;
            }
            x = next;
        }
    }

    /// The faster root is the floor root at every magnitude, at perfect squares and on both
    /// sides of them, so no movement result changes with it.
    #[test]
    fn integer_sqrt_equals_the_newton_root_bit_for_bit() {
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        for index in 0..200_000_u32 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let wide = (u128::from(state) << 64) | u128::from(state.rotate_left(29));
            let value = wide >> (index % 128);
            assert_eq!(integer_sqrt(value), newton_sqrt(value), "value {value}");
        }
        for root in [
            1_u128,
            3,
            (1 << 32) - 1,
            1 << 32,
            (1 << 40) + 7,
            (1 << 63) + 5,
        ] {
            for square in [root * root - 1, root * root, root * root + 1] {
                assert_eq!(integer_sqrt(square), newton_sqrt(square), "value {square}");
            }
        }
        let top = (1_u128 << 64) - 1;
        for value in [
            0,
            1,
            2,
            1 << 64,
            (1 << 64) - 1,
            (1 << 64) + 1,
            top * top,
            u128::MAX,
        ] {
            assert_eq!(integer_sqrt(value), newton_sqrt(value), "value {value}");
        }
    }
}
