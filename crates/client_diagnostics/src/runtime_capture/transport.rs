//! Capture-only evidence for real transport transactions and publication boundaries.
//!
//! This adapter selects existing scenario actors, invokes public typed simulation APIs,
//! and observes the normal client timeline/ECS. Its owned sets and evidence allocations
//! are verification overhead, never part of production synchronization measurements.

use super::config::{CaptureError, Goal, TransportCaptureSpec};
use bevy::prelude::*;
use serde::Serialize;
use sim_core::{transport::TransportReceipt, PlayerResources, SimWorld};
use sim_types::{CombatEventVisual, EntityId, SimVec2};
use std::collections::BTreeSet;

fn failure(message: impl Into<String>) -> CaptureError {
    CaptureError::Verification(message.into())
}

/// Serializable evidence of one verified transport checkpoint in its enclosing tick/generation.
/// Raw positions are deterministic Q32.32 world X/Y; pixels are physical window X/Y.
#[derive(Debug, Clone, Serialize)]
pub(super) struct TransportProof {
    /// Actual selected passenger `EntityId`, not an archetype index.
    pub passenger: u32,
    /// Actual selected carrier `EntityId`, retained after unloading.
    pub carrier: u32,
    /// Successful typed receipt action; absent for the before checkpoint.
    pub receipt_action: Option<String>,
    /// Original dormant pickup position in raw Q32.32 world X/Y.
    pub pickup_raw: [i64; 2],
    /// Carrier selection-time position in raw Q32.32 world X/Y.
    pub carrier_start_raw: [i64; 2],
    /// Authored exact unload endpoint in raw Q32.32 world X/Y.
    pub destination_raw: [i64; 2],
    /// Actual capture-time passenger position, including dormant cargo position.
    pub passenger_raw: [i64; 2],
    /// Actual capture-time carrier position in raw Q32.32 world X/Y.
    pub carrier_raw: [i64; 2],
    /// Sorted expected live noncargo `EntityIds` after exactly this transaction.
    pub expected_members: Vec<u32>,
    /// Sorted `EntityIds` actually displayed by the exact-current timeline.
    pub published_members: Vec<u32>,
    /// Sorted `EntityIds` found in real `LiveBattleBubble` ECS instances.
    pub rendered_members: Vec<u32>,
    /// Selected owner supply: used, capped capacity, uncapped provided total.
    pub supply: [u32; 3],
    /// Authoritative tick immediately before the typed transaction; zero for before.
    pub transaction_tick: u32,
    /// Actual normal-driver publication tick; one after the transaction.
    pub publication_tick: u32,
    /// Distinct sequential displayed ticks observed while checking for shatter.
    pub observed_display_ticks: Vec<u32>,
    /// Actual visible passenger center in physical pixels, absent only while loaded.
    pub passenger_pixel: Option<[f32; 2]>,
    /// Actual visible carrier center in physical window pixels.
    pub carrier_pixel: [f32; 2],
    /// Original pickup projected onto current real terrain/camera in physical pixels.
    pub pickup_pixel: [f32; 2],
    /// Exact endpoint projected onto current real terrain/camera in physical pixels.
    pub destination_pixel: [f32; 2],
    /// Observed shatter events; successful evidence always zero, any occurrence fails.
    pub shatter_events: usize,
    /// Observed real shatter ECS shards; any occurrence fails the checkpoint.
    pub shatter_shards: usize,
}

/// Capture-owned participant identities and expected lifecycle, never authoritative game state.
/// The ordinal selection is fixed once; rejected placement never selects another actor/endpoint.
pub(super) struct TransportState {
    passenger: EntityId,
    carrier: EntityId,
    pickup: SimVec2,
    carrier_start: SimVec2,
    destination: SimVec2,
    owner: usize,
    economy: [PlayerResources; 8],
    initial_members: BTreeSet<EntityId>,
    expected_members: BTreeSet<EntityId>,
    receipt: Option<TransportReceipt>,
    transaction_tick: u32,
    observed_display_ticks: Vec<u32>,
    camera_pose: Option<([f32; 2], f32, [f32; 3])>,
}

fn raw(position: SimVec2) -> [i64; 2] {
    [position.x.raw(), position.y.raw()]
}

fn members(simulation: &SimWorld) -> BTreeSet<EntityId> {
    simulation
        .entities
        .iter()
        .filter(|(_, actor)| actor.is_alive() && !actor.is_loaded_in_transport())
        .map(|(&id, _)| id)
        .collect()
}

impl TransportState {
    /// Resolves authored ordinals once from sorted same-owner living/completed actors
    /// whose entity flags permit motion and which have no foundation/dome shape.
    /// Selected actors must additionally have positive authored maximum speed;
    /// malformed speed fails rather than shifting ordinals to replacement actors.
    /// Returns typed verification evidence for missing identities/definitions/eligibility.
    pub(super) fn select(
        simulation: &SimWorld,
        spec: &TransportCaptureSpec,
    ) -> Result<Self, CaptureError> {
        let mut candidates = Vec::new();
        for (&id, actor) in &simulation.entities {
            if actor.owner.0 != spec.owner
                || !actor.is_alive()
                || !actor.is_completed
                || actor.is_immobile
                || simulation.map.structures().contains_key(&id)
            {
                continue;
            }
            let arch = simulation
                .catalog
                .get(actor.unit_type_id)
                .map_err(|error| failure(format!("transport selector catalog: {error}")))?;
            if !matches!(arch.shape, sim_types::ShapeKind::BuildingDome { .. }) {
                candidates.push((id, actor.position));
            }
        }
        let &(passenger, pickup) = candidates.get(spec.passenger_ordinal).ok_or_else(|| {
            failure("transport passenger ordinal is absent from the actual scenario")
        })?;
        let &(carrier, carrier_start) = candidates.get(spec.carrier_ordinal).ok_or_else(|| {
            failure("transport carrier ordinal is absent from the actual scenario")
        })?;
        if passenger == carrier {
            return Err(failure("transport participants must be distinct"));
        }
        for id in [passenger, carrier] {
            let actor = simulation
                .entities
                .get(&id)
                .ok_or_else(|| failure("selected transport actor missing"))?;
            let arch = simulation
                .catalog
                .get(actor.unit_type_id)
                .map_err(|error| failure(error.to_string()))?;
            if arch.max_speed <= sim_types::SimScalar::ZERO {
                return Err(failure(format!(
                    "selected transport actor {id:?} has no positive authored movement speed"
                )));
            }
        }
        let initial_members = members(simulation);
        if !initial_members.contains(&passenger) || !initial_members.contains(&carrier) {
            return Err(failure("transport participants must initially be present"));
        }
        Ok(Self {
            passenger,
            carrier,
            pickup,
            carrier_start,
            destination: SimVec2::from_i32(spec.destination[0], spec.destination[1]),
            owner: usize::from(spec.owner),
            economy: simulation.player_resources,
            expected_members: initial_members.clone(),
            initial_members,
            receipt: None,
            transaction_tick: simulation.current_tick,
            observed_display_ticks: Vec::new(),
            camera_pose: None,
        })
    }

    /// Returns the retained real carrier identity used by generic visibility verification.
    pub(super) const fn carrier(&self) -> EntityId {
        self.carrier
    }

    /// Invokes one actual public load/unload transaction and records its receipt.
    /// Rejects changed economy or membership beyond exactly the selected passenger.
    /// No simulation tick, placement search or renderer mutation is performed here.
    pub(super) fn apply(
        &mut self,
        simulation: &mut SimWorld,
        goal: Goal,
    ) -> Result<(), CaptureError> {
        self.transaction_tick = simulation.current_tick;
        let economy_before = simulation.player_resources;
        let receipt = match goal {
            Goal::CargoLoaded => {
                simulation.try_load_unit_into_transport(self.passenger, self.carrier)
            }
            Goal::CargoUnloaded => {
                simulation.try_unload_unit_from_transport(self.passenger, self.destination)
            }
            _ => return Err(failure("unsupported transport transaction goal")),
        }
        .map_err(|error| failure(format!("real transport transaction rejected: {error}")))?;
        if receipt.passenger != self.passenger || receipt.carrier != self.carrier {
            return Err(failure(
                "transport receipt identities differ from selected participants",
            ));
        }
        if simulation.player_resources != economy_before {
            return Err(failure(
                "transport transaction changed captured player economy",
            ));
        }
        let next = members(simulation);
        let (removed, added) = if goal == Goal::CargoLoaded {
            (vec![self.passenger], Vec::new())
        } else {
            (Vec::new(), vec![self.passenger])
        };
        if self
            .expected_members
            .difference(&next)
            .copied()
            .collect::<Vec<_>>()
            != removed
            || next
                .difference(&self.expected_members)
                .copied()
                .collect::<Vec<_>>()
                != added
        {
            return Err(failure(
                "transport changed membership beyond the selected passenger",
            ));
        }
        self.expected_members = next;
        self.receipt = Some(receipt);
        // Exact endpoint/link checks run after the normal publication tick.
        let actor = simulation
            .entities
            .get(&self.passenger)
            .ok_or_else(|| failure("transaction passenger missing"))?;
        if actor.transport_carrier() != (goal == Goal::CargoLoaded).then_some(self.carrier)
            || (goal == Goal::CargoUnloaded && actor.position != self.destination)
        {
            return Err(failure("committed transaction does not match its receipt"));
        }
        Ok(())
    }

    fn verify_simulation(&self, simulation: &SimWorld, goal: Goal) -> Result<(), CaptureError> {
        let passenger = simulation
            .entities
            .get(&self.passenger)
            .ok_or_else(|| failure("transport passenger disappeared"))?;
        let carrier = simulation
            .entities
            .get(&self.carrier)
            .ok_or_else(|| failure("transport carrier disappeared"))?;
        if !passenger.is_alive()
            || !carrier.is_alive()
            || members(simulation) != self.expected_members
        {
            return Err(failure(
                "transport publication changed expected live membership",
            ));
        }
        let loaded = goal == Goal::CargoLoaded;
        if passenger.transport_carrier() != loaded.then_some(self.carrier)
            || passenger.position
                != if goal == Goal::CargoUnloaded {
                    self.destination
                } else {
                    self.pickup
                }
        {
            return Err(failure(
                "transport link or exact passenger endpoint changed before capture",
            ));
        }
        if goal == Goal::CargoUnloaded && self.expected_members != self.initial_members {
            return Err(failure(
                "unload did not restore exactly the original roster",
            ));
        }
        let expected_tick = self.transaction_tick + u32::from(goal != Goal::CargoBefore);
        if simulation.current_tick != expected_tick {
            return Err(failure(format!("transport must publish exactly one normal tick: expected {expected_tick}, observed {}", simulation.current_tick)));
        }
        let resource = simulation.player_resources[self.owner];
        let initial = self.economy[self.owner];
        if (
            resource.supply_used,
            resource.supply_max,
            resource.supply_provided_total,
        ) != (
            initial.supply_used,
            initial.supply_max,
            initial.supply_provided_total,
        ) {
            return Err(failure("transport publication changed captured supply"));
        }
        Ok(())
    }

    /// Called every capture observation, including delayed publication and readback.
    pub(super) fn observe_effects(&mut self, world: &mut World) -> Result<(), CaptureError> {
        let events = world.resource::<Events<client::vfx::CombatVisualEvent>>();
        if events
            .iter_current_update_events()
            .any(|event| matches!(event.0, CombatEventVisual::BubbleShatter { .. }))
        {
            return Err(failure("transport observation emitted a shatter event"));
        }
        if world
            .query_filtered::<Entity, With<client::vfx::ShatterShard>>()
            .iter(world)
            .next()
            .is_some()
        {
            return Err(failure("transport observation contains shatter shards"));
        }
        let tick = world
            .resource::<client::battle_pipeline::session::BattleSession>()
            .frames
            .presented()
            .tick
            .0;
        if self.observed_display_ticks.last() != Some(&tick) {
            self.observed_display_ticks.push(tick);
        }
        Ok(())
    }

    /// Verifies the authoritative relation and exact current/presented/ECS membership.
    /// `projected` contains actual visible, media-validated centers in physical pixels;
    /// `region` is normalized window ROI, `resolution` physical pixels, and `pose`
    /// is world X/Z focus, world-unit camera distance and XYZ camera translation.
    /// Returns complete artifact evidence or a typed error without accepting an image.
    #[expect(
        clippy::cast_precision_loss,
        reason = "capture coordinates are bounded by the window size and frame budget"
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "a single ordered sequence that shares its local state"
    )]
    pub(super) fn verify(
        &mut self,
        world: &mut World,
        goal: Goal,
        projected: &[(Option<u32>, Vec2)],
        region: [f32; 4],
        resolution: [u32; 2],
        pose: ([f32; 2], f32, [f32; 3]),
    ) -> Result<TransportProof, CaptureError> {
        let simulation = world
            .resource::<client::battle_pipeline::session::BattleSession>()
            .world()
            .ok_or_else(|| failure("transport world missing"))?;
        self.verify_simulation(simulation, goal)?;
        let passenger_raw = raw(simulation.entities[&self.passenger].position);
        let carrier_raw = raw(simulation.entities[&self.carrier].position);
        let resource = simulation.player_resources[self.owner];
        let tick = simulation.current_tick;
        let frames = &world
            .resource::<client::battle_pipeline::session::BattleSession>()
            .frames;
        if frames.current().tick.0 != tick || frames.presented().tick.0 != tick {
            return Err(failure(
                "transport membership is not displayed at the exact current boundary",
            ));
        }
        let current = frames
            .current()
            .bubbles
            .iter()
            .map(|actor| actor.entity_id)
            .collect::<BTreeSet<_>>();
        let published = frames
            .presented()
            .bubbles
            .iter()
            .map(|actor| actor.entity_id)
            .collect::<BTreeSet<_>>();
        let rendered = world
            .query::<&client::LiveBattleBubble>()
            .iter(world)
            .map(|actor| actor.entity_id)
            .collect::<BTreeSet<_>>();
        if current != self.expected_members
            || published != self.expected_members
            || rendered != self.expected_members
        {
            return Err(failure(
                "transport published/ECS membership differs from the exact expected set",
            ));
        }
        if self.camera_pose.is_some_and(|previous| {
            previous.0.map(f32::to_bits) != pose.0.map(f32::to_bits)
                || previous.1.to_bits() != pose.1.to_bits()
                || previous.2.map(f32::to_bits) != pose.2.map(f32::to_bits)
        }) {
            return Err(failure(
                "transport camera changed between membership captures",
            ));
        }
        self.camera_pose = Some(pose);
        let [x, y, w, h] = region;
        let inside = |p: Vec2| {
            p.x >= x * resolution[0] as f32
                && p.x <= (x + w) * resolution[0] as f32
                && p.y >= y * resolution[1] as f32
                && p.y <= (y + h) * resolution[1] as f32
        };
        let pixel = |id: EntityId| {
            projected
                .iter()
                .find(|(candidate, p)| *candidate == Some(id.0) && inside(*p))
                .map(|(_, p)| p.to_array())
        };
        let carrier_pixel = pixel(self.carrier).ok_or_else(|| {
            failure("transport carrier is not visibly inside the gameplay viewport")
        })?;
        let passenger_pixel = pixel(self.passenger);
        if goal != Goal::CargoLoaded && passenger_pixel.is_none() {
            return Err(failure(
                "transport passenger is not visibly inside the gameplay viewport",
            ));
        }
        let pickup_pixel = project_position(world, self.pickup)?;
        let destination_pixel = project_position(world, self.destination)?;
        if !inside(Vec2::from_array(pickup_pixel)) || !inside(Vec2::from_array(destination_pixel)) {
            return Err(failure(
                "transport pickup/endpoint lies outside the fixed gameplay viewport",
            ));
        }
        Ok(TransportProof {
            passenger: self.passenger.0,
            carrier: self.carrier.0,
            receipt_action: self.receipt.map(|receipt| format!("{:?}", receipt.action)),
            pickup_raw: raw(self.pickup),
            carrier_start_raw: raw(self.carrier_start),
            destination_raw: raw(self.destination),
            passenger_raw,
            carrier_raw,
            expected_members: self.expected_members.iter().map(|id| id.0).collect(),
            published_members: published.iter().map(|id| id.0).collect(),
            rendered_members: rendered.iter().map(|id| id.0).collect(),
            supply: [
                resource.supply_used,
                resource.supply_max,
                resource.supply_provided_total,
            ],
            transaction_tick: self.transaction_tick,
            publication_tick: tick,
            observed_display_ticks: self.observed_display_ticks.clone(),
            passenger_pixel,
            carrier_pixel,
            pickup_pixel,
            destination_pixel,
            shatter_events: 0,
            shatter_shards: 0,
        })
    }
}

fn project_position(world: &mut World, position: SimVec2) -> Result<[f32; 2], CaptureError> {
    let simulation = world
        .resource::<client::battle_pipeline::session::BattleSession>()
        .world()
        .ok_or_else(|| failure("transport world missing"))?;
    let height = simulation.map.terrain_altitude_at(position);
    let point = Vec3::new(position.x.to_f32(), height, position.y.to_f32());
    let mut cameras = world.query_filtered::<(&Camera, &GlobalTransform), With<Camera3d>>();
    let (camera, transform) = cameras
        .get_single(world)
        .map_err(|error| failure(error.to_string()))?;
    let coordinates = client::input_policy::ViewportCoordinates::from_camera(camera)
        .ok_or_else(|| failure("transport camera viewport missing"))?;
    client::input_policy::world_to_window(camera, transform, point)
        .map(|p| coordinates.physical_from_window(p).to_array())
        .ok_or_else(|| failure("transport point cannot be projected by actual camera"))
}

#[cfg(test)]
mod tests;
