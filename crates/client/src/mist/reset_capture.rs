//! Opt-in real-render owner replacement audit for the mist capture harness.
//!
//! The fixture replaces actors once without creating mesh/material assets, freezes the
//! first empty-history publication, and correlates main-world ownership with actual
//! queued GPU generations. Bounded audit storage exists only in this nonbenchmark mode.

use super::{benchmark_capture, CaptureBubble, CaptureCase, CaptureOptions, CaptureProgress};
use crate::mist::gpu::{GpuAccounting, GpuResources, PublishedSlot};
use crate::mist::upload::MistGpuIdentity;
use crate::mist::{MistMotion, MistPool, MistRibbon, MistStats, MistWakeMaterial};
use crate::{BubbleDomeMaterial, BubbleInstance};
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::prelude::*;
use bevy::render::primitives::Aabb;
use bevy::render::render_phase::{PhaseItem, ViewSortedRenderPhases};
use bevy::render::render_resource::BufferId;
use bevy::render::RenderApp;
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

const ACTORS: usize = 400;
const MAX_FRAMES: usize = 2048;
const MAX_SLOTS: usize = 4096;
const MAX_STAGING: usize = 8;

#[derive(Debug, Error)]
pub(crate) enum ResetError {
    #[error("owner-reset verification requires nonbenchmark Crowd")]
    InvalidOptions,
    #[error("owner-reset audit violated invariant: {0}")]
    Invariant(&'static str),
    #[error("owner-reset audit storage exhausted: {0}")]
    Capacity(&'static str),
    #[error("owner-reset audit lock poisoned")]
    Poisoned,
    #[error("owner-reset report failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("owner-reset report serialization failed: {0}")]
    Ron(#[from] ron::Error),
}

pub(crate) fn validate(options: &CaptureOptions) -> Result<(), ResetError> {
    if options.verify_owner_reset
        && (options.case != CaptureCase::Crowd || options.benchmark_emitters.is_some())
    {
        return Err(ResetError::InvalidOptions);
    }
    Ok(())
}

#[derive(Clone, Copy, Serialize)]
struct Owner {
    slot: usize,
    generation: u32,
    entity: u64,
    history_seconds: f64,
}

#[derive(Serialize)]
struct MainFrame {
    sequence: u64,
    trajectory_frame: u32,
    pool_seconds: f32,
    owner_start: usize,
    owner_count: usize,
    actor_bounds_inside: usize,
    pending_and_published_bounds_inside: usize,
    visible_ribbons: usize,
    history_samples: usize,
}

#[derive(Serialize)]
struct QueueFrame {
    main_sequence: u64,
    packet_sequence: u64,
    selected_fresh_packet: bool,
    slot_start: usize,
    slot_count: usize,
    draw_start: usize,
    draw_count: usize,
    views: usize,
}

#[derive(Serialize)]
struct PacketSlot {
    slot: usize,
    generation: u32,
    samples: u32,
    origin: [f32; 3],
}

#[derive(Serialize)]
struct QueuedDraw {
    view: u64,
    slot: u32,
    generation: u32,
    distance: f32,
}

#[derive(Serialize)]
struct ActorIdentity {
    index: usize,
    entity: u64,
    mesh: String,
    material: String,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BufferIdentities {
    pub targets: [BufferId; 3],
    pub staging: [Option<BufferId>; MAX_STAGING],
}

#[derive(Serialize)]
struct Report {
    format_version: u32,
    reset_main_sequence: Option<u64>,
    reset_trajectory_frame: Option<u32>,
    checkpoint_sequences: [Option<u64>; 3],
    before_actors: Vec<ActorIdentity>,
    after_actors: Vec<ActorIdentity>,
    topology_meshes: Vec<String>,
    ribbon_materials: Vec<String>,
    buffer_identities: Option<String>,
    accounting_before: Option<GpuAccounting>,
    accounting_after: Option<GpuAccounting>,
    main_frames: Vec<MainFrame>,
    owners: Vec<Owner>,
    queue_frames: Vec<QueueFrame>,
    packet_slots: Vec<PacketSlot>,
    queued_draws: Vec<QueuedDraw>,
}

struct Audit {
    active: bool,
    replaced: bool,
    finished: bool,
    error: Option<String>,
    buffers: Option<BufferIdentities>,
    mesh_ids: Vec<AssetId<Mesh>>,
    material_ids: Vec<AssetId<MistWakeMaterial>>,
    render_entities: Vec<Entity>,
    asset_counts: Option<(usize, usize, usize)>,
    report: Report,
}

impl Audit {
    fn new() -> Self {
        // PERFORMANCE RATIONALE:
        // Why: A reset audit must observe every queue transition without per-frame growth.
        // What: Reserve bounded flat records before the app starts; failures never truncate.
        // Trade-off: Verification-only memory scales with 2048 frames and 400 actual owners.
        Self {
            active: false,
            replaced: false,
            finished: false,
            error: None,
            buffers: None,
            mesh_ids: Vec::new(),
            material_ids: Vec::new(),
            render_entities: Vec::new(),
            asset_counts: None,
            report: Report {
                format_version: 1,
                reset_main_sequence: None,
                reset_trajectory_frame: None,
                checkpoint_sequences: [None; 3],
                before_actors: Vec::new(),
                after_actors: Vec::new(),
                topology_meshes: Vec::new(),
                ribbon_materials: Vec::new(),
                buffer_identities: None,
                accounting_before: None,
                accounting_after: None,
                main_frames: Vec::with_capacity(MAX_FRAMES),
                owners: Vec::with_capacity(MAX_FRAMES * ACTORS),
                queue_frames: Vec::with_capacity(MAX_FRAMES),
                packet_slots: Vec::with_capacity(MAX_FRAMES * ACTORS),
                queued_draws: Vec::with_capacity(MAX_FRAMES * ACTORS),
            },
        }
    }

    fn validate_complete(&self) -> Result<(), ResetError> {
        let report = &self.report;
        let reset = report
            .reset_main_sequence
            .ok_or(ResetError::Invariant("no reset publication"))?;
        if report.before_actors.len() != ACTORS || report.after_actors.len() != ACTORS {
            return Err(ResetError::Invariant("replacement actor count"));
        }
        let reset_main = report
            .main_frames
            .iter()
            .find(|frame| frame.sequence == reset)
            .ok_or(ResetError::Invariant("missing reset main frame"))?;
        if reset_main.history_samples != 0 || reset_main.visible_ribbons != 0 {
            return Err(ResetError::Invariant(
                "reset must hide all old histories immediately",
            ));
        }
        let previous = report
            .main_frames
            .iter()
            .rev()
            .find(|frame| frame.sequence < reset)
            .ok_or(ResetError::Invariant("missing pre-reset ownership"))?;
        let prior_owners =
            &report.owners[previous.owner_start..previous.owner_start + previous.owner_count];
        let reset_owners =
            &report.owners[reset_main.owner_start..reset_main.owner_start + reset_main.owner_count];
        for owner in reset_owners {
            let prior = prior_owners
                .binary_search_by_key(&owner.slot, |value| value.slot)
                .map_err(|_| ResetError::Invariant("replacement did not reuse occupied slot"))?;
            if prior_owners[prior].generation.checked_add(1) != Some(owner.generation)
                || owner.history_seconds.to_bits() != 0.0_f64.to_bits()
                || !report
                    .after_actors
                    .iter()
                    .any(|actor| actor.entity == owner.entity)
            {
                return Err(ResetError::Invariant(
                    "replacement generation, owner or local history epoch",
                ));
            }
        }
        let reset_packet = report
            .queue_frames
            .iter()
            .find(|frame| frame.packet_sequence == reset)
            .ok_or(ResetError::Invariant("first reset packet was never queued"))?;
        if reset_packet.draw_count != 0
            || report.packet_slots
                [reset_packet.slot_start..reset_packet.slot_start + reset_packet.slot_count]
                .iter()
                .any(|slot| slot.samples != 0)
        {
            return Err(ResetError::Invariant(
                "reset packet contains stale samples or draws",
            ));
        }
        let final_sequence = report.checkpoint_sequences[2]
            .ok_or(ResetError::Invariant("final checkpoint absent"))?;
        let final_frame = report
            .queue_frames
            .iter()
            .find(|frame| frame.main_sequence == final_sequence)
            .ok_or(ResetError::Invariant("final captured render frame absent"))?;
        if final_frame.draw_count != ACTORS || final_frame.views != 1 {
            return Err(ResetError::Invariant(
                "not all replacement wakes actually queued in final view",
            ));
        }
        if !report
            .queue_frames
            .iter()
            .any(|frame| frame.main_sequence > reset && frame.draw_count > 0)
        {
            return Err(ResetError::Invariant("no new-generation draws"));
        }
        let before = report
            .accounting_before
            .ok_or(ResetError::Invariant("initial accounting absent"))?;
        let after = report
            .accounting_after
            .ok_or(ResetError::Invariant("final accounting absent"))?;
        if (
            before.target_buffers,
            before.target_bytes,
            before.staging_buffers,
            before.staging_bytes,
        ) != (
            after.target_buffers,
            after.target_bytes,
            after.staging_buffers,
            after.staging_bytes,
        ) {
            return Err(ResetError::Invariant("owned buffer allocations changed"));
        }
        Ok(())
    }
}

#[derive(Resource, Clone)]
pub(crate) struct ResetAudit(Arc<Mutex<Audit>>);

impl ResetAudit {
    pub(crate) fn failed(&self) -> Result<(), ResetError> {
        let audit = self.0.lock().map_err(|_| ResetError::Poisoned)?;
        if audit.error.is_some() {
            Err(ResetError::Invariant(
                "earlier audit failure; see original diagnostic",
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn checkpoint(&self, image: usize, sequence: u64) -> Result<(), ResetError> {
        let mut audit = self.0.lock().map_err(|_| ResetError::Poisoned)?;
        let entry = audit
            .report
            .checkpoint_sequences
            .get_mut(image)
            .ok_or(ResetError::Invariant("checkpoint index"))?;
        if entry.replace(sequence).is_some() {
            return Err(ResetError::Invariant("duplicate checkpoint"));
        }
        Ok(())
    }

    pub(crate) fn finish(&self, directory: &Path) -> Result<(), ResetError> {
        let mut audit = self.0.lock().map_err(|_| ResetError::Poisoned)?;
        if audit.finished {
            return Ok(());
        }
        if audit.error.is_some() {
            return Err(ResetError::Invariant("earlier audit failure"));
        }
        audit.validate_complete()?;
        std::fs::write(
            directory.join("owner-reset.ron"),
            ron::ser::to_string_pretty(&audit.report, ron::ser::PrettyConfig::default())?,
        )?;
        audit.finished = true;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_queue(
        &self,
        main_sequence: u64,
        packet_sequence: u64,
        selected: bool,
        slots: &[PublishedSlot],
        phases: &ViewSortedRenderPhases<Transparent3d>,
        identities: &Query<'_, '_, &MistGpuIdentity>,
        buffers: BufferIdentities,
    ) -> Result<(), ResetError> {
        let mut audit = self.0.lock().map_err(|_| ResetError::Poisoned)?;
        if !audit.active || audit.finished {
            return Ok(());
        }
        if let Some(previous) = audit.buffers {
            if previous != buffers {
                return Err(ResetError::Invariant(
                    "actual target/staging buffer identities changed",
                ));
            }
        } else {
            audit.report.buffer_identities = Some(format!(
                "targets={:?}; staging={:?}",
                buffers.targets, buffers.staging
            ));
            audit.buffers = Some(buffers);
        }
        let main_index = audit
            .report
            .main_frames
            .binary_search_by_key(&main_sequence, |frame| frame.sequence)
            .map_err(|_| {
                ResetError::Invariant("render packet lacks same-sequence main-world evidence")
            })?;
        let main = &audit.report.main_frames[main_index];
        let owner_start = main.owner_start;
        let owner_count = main.owner_count;
        if slots.len() > MAX_SLOTS || audit.report.queue_frames.len() == MAX_FRAMES {
            return Err(ResetError::Capacity("render frames or configured slots"));
        }
        let slot_start = audit.report.packet_slots.len();
        let draw_start = audit.report.queued_draws.len();
        for index in 0..owner_count {
            let owner = audit.report.owners[owner_start + index];
            let slot = slots
                .get(owner.slot)
                .ok_or(ResetError::Invariant("packet slot absent"))?;
            bounded_push(
                &mut audit.report.packet_slots,
                PacketSlot {
                    slot: owner.slot,
                    generation: slot.generation,
                    samples: slot.count,
                    origin: slot.origin.to_array(),
                },
                "packet slots",
            )?;
        }
        let mut views = 0;
        for (view, phase) in phases.iter() {
            views += 1;
            for item in &phase.items {
                let Ok(identity) = identities.get(item.entity()) else {
                    continue;
                };
                let owners = &audit.report.owners[owner_start..owner_start + owner_count];
                let owner_index = owners
                    .binary_search_by_key(&(identity.slot as usize), |owner| owner.slot)
                    .map_err(|_| ResetError::Invariant("queued slot has no current owner"))?;
                let owner = &owners[owner_index];
                if identity.generation != owner.generation {
                    return Err(ResetError::Invariant(
                        "retired owner generation actually queued",
                    ));
                }
                bounded_push(
                    &mut audit.report.queued_draws,
                    QueuedDraw {
                        view: view.to_bits(),
                        slot: identity.slot,
                        generation: identity.generation,
                        distance: item.distance,
                    },
                    "queued draws",
                )?;
            }
        }
        let draw_count = audit.report.queued_draws.len() - draw_start;
        audit.report.queue_frames.push(QueueFrame {
            main_sequence,
            packet_sequence,
            selected_fresh_packet: selected,
            slot_start,
            slot_count: owner_count,
            draw_start,
            draw_count,
            views,
        });
        Ok(())
    }
}

fn bounded_push<T>(entries: &mut Vec<T>, value: T, name: &'static str) -> Result<(), ResetError> {
    if entries.len() == entries.capacity() {
        return Err(ResetError::Capacity(name));
    }
    entries.push(value);
    Ok(())
}

pub(super) fn install(app: &mut App) {
    let audit = ResetAudit(Arc::new(Mutex::new(Audit::new())));
    app.insert_resource(audit.clone())
        .add_systems(Update, replace_owners.after(super::move_capture_bubbles))
        .add_systems(Last, record_main.before(super::capture_frames));
    app.sub_app_mut(RenderApp).insert_resource(audit);
}

fn reset_due(progress: &CaptureProgress, replaced: bool) -> bool {
    progress.running && progress.image == 1 && !progress.pending && !replaced
}

pub(super) fn target_frame(image: usize, fps: u32) -> u32 {
    if image < 2 {
        10 * (fps / 6)
    } else {
        14 * (fps / 6)
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn replace_owners(
    mut commands: Commands<'_, '_>,
    progress: Res<'_, CaptureProgress>,
    time: Res<'_, Time>,
    audit: Res<'_, ResetAudit>,
    actors: Query<
        '_,
        '_,
        (
            Entity,
            &CaptureBubble,
            &BubbleInstance,
            &Transform,
            &Mesh3d,
            &MeshMaterial3d<BubbleDomeMaterial>,
            &MistMotion,
        ),
    >,
    mut exit: EventWriter<'_, AppExit>,
) {
    let result = (|| -> Result<(), ResetError> {
        let mut audit = audit.0.lock().map_err(|_| ResetError::Poisoned)?;
        if !reset_due(&progress, audit.replaced) {
            return Ok(());
        }
        if time.delta() != std::time::Duration::ZERO {
            return Err(ResetError::Invariant(
                "owner replacement must occur at frozen presentation time",
            ));
        }
        if actors.iter().count() != ACTORS {
            return Err(ResetError::Invariant(
                "replacement requires exactly 400 actors",
            ));
        }
        for (entity, fixture, bubble, transform, mesh, material, motion) in &actors {
            audit.report.before_actors.push(actor_identity(
                entity,
                fixture.index,
                transform,
                mesh,
                material,
            ));
            commands.entity(entity).despawn();
            let replacement = commands
                .spawn((
                    CaptureBubble {
                        origin: fixture.origin,
                        index: fixture.index,
                    },
                    bubble.clone(),
                    *transform,
                    mesh.clone(),
                    material.clone(),
                    *motion,
                ))
                .id();
            audit.report.after_actors.push(actor_identity(
                replacement,
                fixture.index,
                transform,
                mesh,
                material,
            ));
        }
        audit
            .report
            .before_actors
            .sort_unstable_by_key(|actor| actor.index);
        audit
            .report
            .after_actors
            .sort_unstable_by_key(|actor| actor.index);
        validate_replacement(&audit.report.before_actors, &audit.report.after_actors)?;
        audit.replaced = true;
        audit.report.reset_trajectory_frame = Some(progress.elapsed_frames);
        Ok(())
    })();
    if let Err(error) = result {
        fail(&audit, error, &mut exit);
    }
}

fn actor_identity(
    entity: Entity,
    index: usize,
    transform: &Transform,
    mesh: &Mesh3d,
    material: &MeshMaterial3d<BubbleDomeMaterial>,
) -> ActorIdentity {
    ActorIdentity {
        index,
        entity: entity.to_bits(),
        mesh: format!("{:?}", mesh.id()),
        material: format!("{:?}", material.id()),
        translation: transform.translation.to_array(),
        rotation: transform.rotation.to_array(),
        scale: transform.scale.to_array(),
    }
}

fn validate_replacement(
    before: &[ActorIdentity],
    after: &[ActorIdentity],
) -> Result<(), ResetError> {
    for (old, new) in before.iter().zip(after) {
        if old.index != new.index
            || old.entity == new.entity
            || before.iter().any(|actor| actor.entity == new.entity)
            || old.mesh != new.mesh
            || old.material != new.material
            || old.translation.map(f32::to_bits) != new.translation.map(f32::to_bits)
            || old.rotation.map(f32::to_bits) != new.rotation.map(f32::to_bits)
            || old.scale.map(f32::to_bits) != new.scale.map(f32::to_bits)
        {
            return Err(ResetError::Invariant(
                "replacement changed pose/assets or retained an actor identity",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn record_main(
    options: Res<'_, CaptureOptions>,
    progress: Res<'_, CaptureProgress>,
    audit: Res<'_, ResetAudit>,
    pool: Res<'_, MistPool>,
    stats: Res<'_, MistStats>,
    resources: Res<'_, GpuResources>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform)>,
    actors: Query<
        '_,
        '_,
        (
            Entity,
            &CaptureBubble,
            &BubbleInstance,
            &GlobalTransform,
            &Transform,
            &Mesh3d,
            &MeshMaterial3d<BubbleDomeMaterial>,
        ),
    >,
    ribbons: Query<
        '_,
        '_,
        (
            &Aabb,
            &GlobalTransform,
            &Visibility,
            &MeshMaterial3d<MistWakeMaterial>,
        ),
        With<MistRibbon>,
    >,
    meshes: Res<'_, Assets<Mesh>>,
    materials: Res<'_, Assets<MistWakeMaterial>>,
    shells: Res<'_, Assets<BubbleDomeMaterial>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let result = (|| -> Result<(), ResetError> {
        let mut audit = audit.0.lock().map_err(|_| ResetError::Poisoned)?;
        if !progress.running
            || progress.elapsed_frames + 1 < target_frame(0, options.fps)
            || audit.finished
            || progress.complete
        {
            return Ok(());
        }
        if audit.error.is_some() {
            return Err(ResetError::Invariant("earlier failure"));
        }
        if pool.slots.len() > MAX_SLOTS || stats.overflows != 0 || stats.occupied != ACTORS {
            return Err(ResetError::Invariant("pool capacity/ownership/overflow"));
        }
        let counts = (meshes.len(), materials.len(), shells.len());
        if !audit.active {
            audit.active = true;
            audit.asset_counts = Some(counts);
            for slot in &pool.slots {
                let (_, _, _, material) = ribbons
                    .get(slot.render_entity)
                    .map_err(|_| ResetError::Invariant("missing render entity"))?;
                audit.mesh_ids.push(slot.mesh.id());
                audit.material_ids.push(material.id());
                audit.render_entities.push(slot.render_entity);
                audit
                    .report
                    .topology_meshes
                    .push(format!("{:?}", slot.mesh.id()));
                audit
                    .report
                    .ribbon_materials
                    .push(format!("{:?}", material.id()));
            }
        }
        if audit.asset_counts != Some(counts) {
            return Err(ResetError::Invariant("mesh/material asset count changed"));
        }
        if audit.replaced && audit.report.reset_main_sequence.is_none() {
            let mut actual = Vec::with_capacity(ACTORS);
            for (entity, fixture, _, _, transform, mesh, material) in &actors {
                actual.push(actor_identity(
                    entity,
                    fixture.index,
                    transform,
                    mesh,
                    material,
                ));
            }
            actual.sort_unstable_by_key(|actor| actor.index);
            if actual.len() != ACTORS
                || actual
                    .iter()
                    .zip(&audit.report.after_actors)
                    .any(|(observed, expected)| observed.entity != expected.entity)
            {
                return Err(ResetError::Invariant(
                    "deferred replacement not applied before extraction",
                ));
            }
            validate_replacement(&audit.report.before_actors, &actual)?;
            audit.report.after_actors = actual;
            audit.report.reset_main_sequence = Some(pool.sequence);
        }
        if audit.report.main_frames.len() == MAX_FRAMES {
            return Err(ResetError::Capacity("main frames"));
        }
        let (camera, camera_transform) = cameras
            .get_single()
            .map_err(|_| ResetError::Invariant("exactly one real camera required"))?;
        let mut actor_count = 0;
        for (_, _, bubble, transform, _, _, _) in &actors {
            let extent = Vec3::splat(bubble.radius);
            benchmark_capture::verify_bounds(
                camera,
                camera_transform,
                transform.translation() - extent,
                transform.translation() + extent,
                Mat4::IDENTITY,
            )
            .map_err(|_| ResetError::Invariant("actor outside frustum"))?;
            actor_count += 1;
        }
        if actor_count != ACTORS {
            return Err(ResetError::Invariant("actual actor count changed"));
        }
        let owner_start = audit.report.owners.len();
        let mut visible = 0;
        let mut bounds_count = 0;
        for (index, slot) in pool.slots.iter().enumerate() {
            let (bounds, transform, visibility, material) = ribbons
                .get(slot.render_entity)
                .map_err(|_| ResetError::Invariant("missing pooled render entity"))?;
            if audit.mesh_ids[index] != slot.mesh.id()
                || audit.material_ids[index] != material.id()
                || audit.render_entities[index] != slot.render_entity
            {
                return Err(ResetError::Invariant("pooled resource identity changed"));
            }
            if let Some(owner) = slot.owner {
                benchmark_capture::verify_ribbon_bounds(
                    camera,
                    camera_transform,
                    bounds,
                    transform,
                )
                .map_err(|_| {
                    ResetError::Invariant("pending/published wake support outside frustum")
                })?;
                bounds_count += 1;
                visible += usize::from(*visibility == Visibility::Visible);
                bounded_push(
                    &mut audit.report.owners,
                    Owner {
                        slot: index,
                        generation: slot.generation,
                        entity: owner.to_bits(),
                        history_seconds: slot.history.clock(),
                    },
                    "owners",
                )?;
            }
        }
        let owner_count = audit.report.owners.len() - owner_start;
        if owner_count != ACTORS {
            return Err(ResetError::Invariant("actual slot ownership count changed"));
        }
        audit.report.main_frames.push(MainFrame {
            sequence: pool.sequence,
            trajectory_frame: progress.elapsed_frames,
            pool_seconds: pool.clock,
            owner_start,
            owner_count,
            actor_bounds_inside: actor_count,
            pending_and_published_bounds_inside: bounds_count,
            visible_ribbons: visible,
            history_samples: stats.samples,
        });
        let accounting = resources
            .publication
            .lock()
            .map_err(|_| ResetError::Poisoned)?
            .accounting;
        audit.report.accounting_before.get_or_insert(accounting);
        audit.report.accounting_after = Some(accounting);
        Ok(())
    })();
    if let Err(error) = result {
        fail(&audit, error, &mut exit);
    }
}

fn fail(audit: &ResetAudit, error: ResetError, exit: &mut EventWriter<'_, AppExit>) {
    error!("{error}");
    if let Ok(mut audit) = audit.0.lock() {
        audit.error = Some(error.to_string());
    }
    exit.send(AppExit::error());
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn option_requires_nonbenchmark_crowd() -> Result<(), Box<dyn std::error::Error>> {
        for args in [
            vec!["capture", "--out", "/tmp/unused", "--verify-owner-reset"],
            vec![
                "capture",
                "--out",
                "/tmp/unused",
                "--case",
                "crowd",
                "--verify-owner-reset",
                "--benchmark-emitters",
                "400",
            ],
        ] {
            let options = CaptureOptions::try_parse_from(args)?;
            assert!(matches!(
                validate(&options),
                Err(ResetError::InvalidOptions)
            ));
        }
        let options = CaptureOptions::try_parse_from([
            "capture",
            "--out",
            "/tmp/unused",
            "--case",
            "crowd",
            "--verify-owner-reset",
        ])?;
        assert!(validate(&options).is_ok());
        Ok(())
    }

    #[test]
    fn reset_waits_for_acknowledgment_and_occurs_once_at_exact_time() {
        let mut progress = CaptureProgress {
            running: true,
            image: 1,
            pending: true,
            ..default()
        };
        assert!(!reset_due(&progress, false));
        progress.pending = false;
        assert!(reset_due(&progress, false));
        assert!(!reset_due(&progress, true));
        for fps in [30, 60, 144] {
            assert_eq!(target_frame(0, fps), target_frame(1, fps));
            assert_eq!(target_frame(2, fps) - target_frame(1, fps), 2 * fps / 3);
        }
    }

    #[test]
    fn actual_capture_clock_stays_frozen_while_reset_pipeline_is_unready(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::time::TimeUpdateStrategy;
        use std::sync::atomic::AtomicBool;
        use std::time::Duration;

        // This is a scheduling/clock test only. It deliberately supplies an unready
        // pipeline state and never renders, loads substitute media, or saves a PNG.
        // The real pipeline and first-reset screenshot remain native acceptance work.
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let pool = world.run_system_once(
            |mut commands: Commands<'_, '_>, mut meshes: ResMut<'_, Assets<Mesh>>| {
                crate::mist::allocate_pool(
                    crate::mist::MistStyle::default(),
                    &mut commands,
                    &mut meshes,
                )
            },
        )??;
        world.insert_resource(pool);
        world.init_resource::<MistStats>();
        world.init_resource::<Time>();
        world.init_resource::<Time<Virtual>>();
        world.init_resource::<Events<AppExit>>();
        world.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
        world.insert_resource(CaptureOptions::try_parse_from([
            "capture",
            "--out",
            "/tmp/unused",
            "--case",
            "crowd",
            "--verify-owner-reset",
        ])?);
        world.insert_resource(CaptureProgress {
            running: true,
            image: 1,
            elapsed_frames: target_frame(1, 60),
            ..default()
        });
        world.insert_resource(super::super::PipelineStatus {
            ready: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(AtomicBool::new(false)),
            mist_shader: Handle::default(),
            bubble_shader: Handle::default(),
            require_mist: true,
        });
        for _ in 0..3 {
            world.run_system_once(super::super::capture_frames)?;
            assert!(matches!(world.resource::<TimeUpdateStrategy>(),
                TimeUpdateStrategy::ManualDuration(value) if *value == Duration::ZERO));
            assert!(!world.resource::<CaptureProgress>().pending);
        }
        // Only the acknowledged empty screenshot advances the fixture to image2.
        world.resource_mut::<CaptureProgress>().image = 2;
        world.run_system_once(super::super::capture_frames)?;
        assert!(matches!(world.resource::<TimeUpdateStrategy>(),
            TimeUpdateStrategy::ManualDuration(value) if *value > Duration::ZERO));
        assert!(world.resource::<Events<AppExit>>().is_empty());
        Ok(())
    }

    #[test]
    fn bounded_evidence_fails_instead_of_growing_or_accepting_incomplete_reset() {
        let mut entries = Vec::with_capacity(1);
        assert!(bounded_push(&mut entries, 1, "test").is_ok());
        assert!(matches!(
            bounded_push(&mut entries, 2, "test"),
            Err(ResetError::Capacity("test"))
        ));
        assert_eq!(entries.capacity(), 1);
        assert!(Audit::new().validate_complete().is_err());
    }
}
