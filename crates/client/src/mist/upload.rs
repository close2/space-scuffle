//! Persistent GPU transfer lifecycle and packet-aligned transparent phase ordering.
//!
//! A complete packet is selected before material queueing, copied before camera draws,
//! and acknowledged only after Bevy submits that render frame. Three-way staging never
//! grows to hide a slow GPU; retained geometry keeps its own ages, origin and frame clock.

use super::benchmark::TransferStage;
use super::gpu::{GpuResources, MistGpuError, Packet, PublishedSlot};
use super::lighting::LightAdmission;
use super::{MistPool, MistStyle, MistWakeMaterial};

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::ecs::query::QueryItem;
use bevy::pbr::queue_material_meshes;
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_graph::{
    Node, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel,
};
use bevy::render::render_phase::{PhaseItem, ViewSortedRenderPhases};
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages, MapMode};
use bevy::render::renderer::{render_system, RenderContext, RenderDevice};
use bevy::render::view::ExtractedView;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSet};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(test)]
pub(super) use tests::verify_publication_and_sorting;

const WRITABLE: u8 = 0;
const COPY_SELECTED: u8 = 1;
const AWAITING_MAP: u8 = 2;
const MAP_FAILED: u8 = 3;
const HELD_MAP: u8 = 4;

/// Explicit shader-slot identity; the geometric transform never encodes ownership.
#[derive(Component, Debug, Clone, Copy)]
pub struct MistGpuIdentity {
    /// Validated fixed emitter-slot index.
    pub slot: u32,
    /// Monotonically checked ownership generation within that slot.
    pub generation: u32,
}

impl ExtractComponent for MistGpuIdentity {
    type QueryData = &'static Self;
    type QueryFilter = ();
    type Out = Self;

    fn extract_component(item: QueryItem<'_, Self::QueryData>) -> Option<Self> {
        Some(*item)
    }
}

struct StagingSlot {
    buffer: Buffer,
    state: Arc<AtomicU8>,
    // Present only for an explicitly requested real-render callback-stall fixture.
    callback_gate: Option<Arc<AtomicBool>>,
    // Tests can defer delivery after a real map callback; no GPU work is skipped.
    #[cfg(test)]
    release_callback: Arc<AtomicBool>,
}

#[derive(Resource)]
struct UploadState {
    resources: GpuResources,
    pending: Packet,
    published: Vec<PublishedSlot>,
    staging: Vec<StagingSlot>,
    next_staging: usize,
    selected: Option<usize>,
    copy_recorded: AtomicBool,
    failed: bool,
    stalled_since: Option<Instant>,
    stall_timeout: Duration,
    timing_enabled: bool,
}

impl UploadState {
    fn new(
        device: &RenderDevice,
        resources: GpuResources,
        style: &MistStyle,
        timing_enabled: bool,
    ) -> Result<Self, MistGpuError> {
        // PERFORMANCE RATIONALE:
        // Why: Queue::write_buffer hides temporary staging allocations on every upload.
        // What: Allocate the complete mapped ring and packet arrays once at initialization.
        // Trade-off: Bounded memory and explicit GPU backpressure replace automatic growth.
        let staging: Vec<_> = (0..style.gpu_staging_buffers)
            .map(|_| StagingSlot {
                buffer: device.create_buffer(&BufferDescriptor {
                    label: Some("mist bounded staging packet"),
                    size: resources.layout.total_bytes,
                    usage: BufferUsages::MAP_WRITE | BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                }),
                state: Arc::new(AtomicU8::new(WRITABLE)),
                callback_gate: None,
                #[cfg(test)]
                release_callback: Arc::new(AtomicBool::new(true)),
            })
            .collect();
        {
            let mut publication = resources
                .publication
                .lock()
                .map_err(|_| MistGpuError::PublicationPoisoned)?;
            let count = staging.len() as u64;
            publication.accounting.staging_buffers = publication
                .accounting
                .staging_buffers
                .checked_add(count)
                .ok_or(MistGpuError::CounterExhausted("staging buffer"))?;
            let bytes = count
                .checked_mul(resources.layout.total_bytes)
                .ok_or(MistGpuError::CounterExhausted("staging byte"))?;
            publication.accounting.staging_bytes = publication
                .accounting
                .staging_bytes
                .checked_add(bytes)
                .ok_or(MistGpuError::CounterExhausted("staging byte"))?;
        }
        Ok(Self {
            pending: Packet::new(resources.layout),
            published: vec![PublishedSlot::default(); resources.layout.emitters],
            resources,
            staging,
            next_staging: 0,
            selected: None,
            copy_recorded: AtomicBool::new(false),
            failed: false,
            stalled_since: None,
            stall_timeout: Duration::from_secs_f32(style.gpu_stall_timeout_seconds),
            timing_enabled,
        })
    }

    fn fail(&mut self, error: MistGpuError) {
        self.failed = true;
        error!("Mist GPU transfer failed: {error}");
        if let Ok(mut publication) = self.resources.publication.lock() {
            publication.error = Some(error);
        } else {
            error!("{}", MistGpuError::PublicationPoisoned);
        }
    }

    fn record_timing(
        &self,
        stage: TransferStage,
        started: Option<Instant>,
    ) -> Result<(), MistGpuError> {
        let Some(started) = started else {
            return Ok(());
        };
        // Stop before locking, so this named stage excludes evidence-publication overhead.
        let elapsed = started.elapsed();
        let mut publication = self
            .resources
            .publication
            .lock()
            .map_err(|_| MistGpuError::PublicationPoisoned)?;
        if let Some(measurements) = &mut publication.measurements {
            measurements
                .record(stage, elapsed)
                .map_err(|_| MistGpuError::MeasurementFailed)?;
        }
        Ok(())
    }

    fn extract(&mut self, pool: &MistPool) -> Result<(), MistGpuError> {
        let started = self.timing_enabled.then(Instant::now);
        self.pack(pool)?;
        self.record_timing(TransferStage::Extraction, started)
    }

    fn pack(&mut self, pool: &MistPool) -> Result<(), MistGpuError> {
        pool.frame.validate()?;
        self.pending.sequence = pool.sequence;
        self.pending.frame = pool.frame;
        for (index, slot) in pool.slots.iter().enumerate() {
            self.pending.write_slot(
                index,
                self.resources.layout,
                &slot.history,
                slot.generation,
                slot.origin,
                slot.offset,
                slot.radius,
                slot.boosted,
                &pool.style,
            )?;
        }
        Ok(())
    }

    fn selected_slots(&self) -> &[PublishedSlot] {
        if self.selected.is_some() {
            &self.pending.slots
        } else {
            &self.published
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct MistUploadLabel;

#[derive(Default)]
struct MistUploadNode;

impl Node for MistUploadNode {
    fn run(
        &self,
        _graph: &mut RenderGraphContext<'_>,
        context: &mut RenderContext<'_>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(state) = world.get_resource::<UploadState>() else {
            return Ok(());
        };
        if state.failed {
            return Ok(());
        }
        let Some(index) = state.selected else {
            return Ok(());
        };
        let started = state.timing_enabled.then(Instant::now);
        record_selected_copy(state, index, context.command_encoder());
        if let Err(error) = state.record_timing(TransferStage::CopyRecording, started) {
            error!("Mist copy recording evidence failed: {error}");
            if let Ok(mut publication) = state.resources.publication.lock() {
                publication.error = Some(error);
            }
        }
        Ok(())
    }
}

fn record_selected_copy(
    state: &UploadState,
    index: usize,
    encoder: &mut bevy::render::render_resource::CommandEncoder,
) {
    let source = &state.staging[index].buffer;
    let layout = state.resources.layout;
    encoder.copy_buffer_to_buffer(source, 0, &state.resources.headers, 0, layout.header_bytes);
    encoder.copy_buffer_to_buffer(
        source,
        layout.header_bytes,
        &state.resources.samples,
        0,
        layout.sample_bytes,
    );
    encoder.copy_buffer_to_buffer(
        source,
        layout.frame_offset,
        &state.resources.frame,
        0,
        size_of::<super::gpu::FrameGpu>() as u64,
    );
    state.copy_recorded.store(true, Ordering::Release);
}

fn aligned_queue_system() -> bevy::ecs::schedule::SystemConfigs {
    align_transparent_packet
        .in_set(RenderSet::QueueMeshes)
        .after(queue_material_meshes::<MistWakeMaterial>)
        // Stable distance sorting retains insertion order for exact ties.
        // A shell and its wake share an origin: preserve shell-over-wake.
        .before(queue_material_meshes::<crate::BubbleDomeMaterial>)
}

pub(super) fn install(app: &mut App) {
    app.add_plugins(ExtractComponentPlugin::<MistGpuIdentity>::default());
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render
        .init_resource::<LightAdmission>()
        .add_systems(ExtractSchedule, extract_packet)
        .add_systems(
            Render,
            (
                select_packet.in_set(RenderSet::ManageViews),
                aligned_queue_system(),
                inspect_failed_light_phase
                    .in_set(RenderSet::PhaseSort)
                    .before(bevy::render::render_phase::sort_phase_system::<Transparent3d>),
                acknowledge_submission
                    .in_set(RenderSet::Render)
                    .after(render_system),
            ),
        );
    let mut graph = render.world_mut().resource_mut::<RenderGraph>();
    graph.add_node(MistUploadLabel, MistUploadNode);
    graph.add_node_edge(MistUploadLabel, bevy::render::graph::CameraDriverLabel);
}

fn extract_packet(
    mut commands: Commands<'_, '_>,
    pool: Extract<'_, '_, Option<Res<'_, MistPool>>>,
    resources: Extract<'_, '_, Option<Res<'_, GpuResources>>>,
    device: Res<'_, RenderDevice>,
    timing: Extract<'_, '_, Option<Res<'_, super::MistCpuTiming>>>,
    state: Option<ResMut<'_, UploadState>>,
    admission: Extract<'_, '_, Res<'_, LightAdmission>>,
    mut render_admission: ResMut<'_, LightAdmission>,
) {
    *render_admission = **admission;
    if !render_admission.admitted() {
        if let (Some(mut state), LightAdmission::Failed(error)) = (state, *render_admission) {
            state.fail(error.into());
        }
        return;
    }
    let (Some(pool), Some(resources)) = (pool.as_ref(), resources.as_ref()) else {
        return;
    };
    if let Some(mut state) = state {
        if !state.failed {
            if let Err(error) = state.extract(pool) {
                state.fail(error);
            }
        }
    } else {
        match UploadState::new(&device, (*resources).clone(), &pool.style, timing.is_some()) {
            Ok(mut state) => {
                if let Err(error) = state.extract(pool) {
                    state.fail(error);
                }
                commands.insert_resource(state);
            }
            Err(error) => {
                error!("Mist GPU initialization failed: {error}");
                if let Ok(mut publication) = resources.publication.lock() {
                    publication.error = Some(error);
                }
            }
        }
    }
}

fn select_packet(
    state: Option<ResMut<'_, UploadState>>,
    gate: Option<Res<'_, super::callback_gate::CallbackGate>>,
    witness: Option<Res<'_, super::packet_witness::PacketWitness>>,
) {
    let Some(mut state) = state else { return };
    if let Some(gate) = &gate {
        for slot in &mut state.staging {
            if slot.callback_gate.is_none() {
                slot.callback_gate = Some(gate.release_flag());
            }
            if gate.released() && slot.state.load(Ordering::Acquire) == HELD_MAP {
                slot.state.store(WRITABLE, Ordering::Release);
            }
        }
    }
    let started = state.timing_enabled.then(Instant::now);
    select_packet_inner(&mut state);
    if let Some(witness) = witness {
        witness.record(
            state.selected.is_some().then_some(&state.pending),
            &state.resources,
            state.staging.iter().map(|slot| slot.buffer.id()),
            state.failed,
        );
    }
    if let Some(gate) = gate {
        let result = gate.record(
            state.selected.is_some().then_some(&state.pending),
            &state.resources,
            state
                .staging
                .iter()
                .filter(|slot| slot.state.load(Ordering::Acquire) == HELD_MAP)
                .count(),
            state.staging.iter().map(|slot| slot.buffer.id()),
        );
        if let Err(error) = result {
            error!("{error}");
            state.fail(MistGpuError::CaptureAuditFailed);
        }
    }
    if let Err(error) = state.record_timing(TransferStage::Selection, started) {
        state.fail(error);
    }
}

fn select_packet_inner(state: &mut UploadState) {
    if state.failed {
        return;
    }
    state.selected = None;
    state.copy_recorded.store(false, Ordering::Release);
    for (index, slot) in state.staging.iter().enumerate() {
        if slot.state.load(Ordering::Acquire) == MAP_FAILED {
            state.fail(MistGpuError::MappingFailed(index));
            return;
        }
    }
    let selected = (0..state.staging.len())
        .map(|offset| (state.next_staging + offset) % state.staging.len())
        .find(|index| state.staging[*index].state.load(Ordering::Acquire) == WRITABLE);
    let Some(index) = selected else {
        let started = *state.stalled_since.get_or_insert_with(Instant::now);
        if started.elapsed() > state.stall_timeout {
            let timeout = state.stall_timeout.as_secs_f32();
            state.fail(MistGpuError::StagingTimeout(timeout));
            return;
        }
        let result = (|| -> Result<(), MistGpuError> {
            let mut publication = state
                .resources
                .publication
                .lock()
                .map_err(|_| MistGpuError::PublicationPoisoned)?;
            publication.accounting.staging_stalls = publication
                .accounting
                .staging_stalls
                .checked_add(1)
                .ok_or(MistGpuError::CounterExhausted("staging stall"))?;
            Ok(())
        })();
        if let Err(error) = result {
            state.fail(error);
        }
        return;
    };
    state.stalled_since = None;
    let layout = state.resources.layout;
    let slot = &state.staging[index];
    // The callback is the only writer returning AWAITING_MAP to WRITABLE. This
    // render thread exclusively owns WRITABLE -> COPY_SELECTED -> AWAITING_MAP.
    slot.state.store(COPY_SELECTED, Ordering::Release);
    {
        let mut bytes = slot.buffer.slice(..).get_mapped_range_mut();
        let samples_offset = layout.header_bytes as usize;
        let frame_offset = layout.frame_offset as usize;
        bytes[..samples_offset].copy_from_slice(bytemuck::cast_slice(&state.pending.headers));
        bytes[samples_offset..frame_offset]
            .copy_from_slice(bytemuck::cast_slice(&state.pending.samples));
        bytes[frame_offset..].copy_from_slice(bytemuck::bytes_of(&state.pending.frame));
    }
    slot.buffer.unmap();
    state.selected = Some(index);
    state.next_staging = (index + 1) % state.staging.len();
}

fn align_transparent_packet(
    state: Option<ResMut<'_, UploadState>>,
    mut phases: ResMut<'_, ViewSortedRenderPhases<Transparent3d>>,
    views: Query<'_, '_, &ExtractedView>,
    identities: Query<'_, '_, &MistGpuIdentity>,
    audit: Option<Res<'_, super::capture::owner_reset::ResetAudit>>,
    admission: Res<'_, LightAdmission>,
) {
    // AppExit is observed later. Suppress only mist in this very render frame,
    // including retained packets and the pre-initialization state with no uploader.
    if !admission.admitted() {
        for phase in phases.values_mut() {
            phase
                .items
                .retain(|item| !identities.contains(item.entity()));
        }
        return;
    }
    let Some(mut state) = state else { return };
    let mut error = None;
    if !state.failed {
        for (view_entity, phase) in phases.iter_mut() {
            let Ok(view) = views.get(*view_entity) else {
                if phase
                    .items
                    .iter()
                    .any(|item| identities.contains(item.entity()))
                {
                    error = Some(MistGpuError::MissingQueuedView);
                    break;
                }
                continue;
            };
            let rangefinder = view.rangefinder3d();
            for item in &mut phase.items {
                let Ok(identity) = identities.get(item.entity()) else {
                    continue;
                };
                let Some(slot) = state.selected_slots().get(identity.slot as usize) else {
                    error = Some(MistGpuError::InvalidQueuedSlot(identity.slot));
                    break;
                };
                if identity.generation != slot.generation {
                    error = Some(MistGpuError::StaleGeneration {
                        slot: identity.slot,
                        draw: identity.generation,
                        packet: slot.generation,
                    });
                    break;
                }
                // Material depth_bias remains zero. Match the baseline emitter-origin
                // policy, including retained packets, independently for every real view.
                item.distance = rangefinder.distance_translation(&slot.origin);
            }
            if error.is_some() {
                break;
            }
        }
    }
    if let Some(error) = error {
        state.fail(error);
    }
    if !state.failed {
        if let Some(audit) = audit {
            let result = (|| -> Result<(), super::capture::owner_reset::ResetError> {
                let packet_sequence = if state.selected.is_some() {
                    state.pending.sequence
                } else {
                    state
                        .resources
                        .publication
                        .lock()
                        .map_err(|_| super::capture::owner_reset::ResetError::Poisoned)?
                        .sequence
                };
                let mut staging = [None; 8];
                for (index, slot) in state.staging.iter().enumerate() {
                    let entry = staging.get_mut(index).ok_or(
                        super::capture::owner_reset::ResetError::Capacity("staging identities"),
                    )?;
                    *entry = Some(slot.buffer.id());
                }
                audit.record_queue(
                    state.pending.sequence,
                    packet_sequence,
                    state.selected.is_some(),
                    state.selected_slots(),
                    &phases,
                    &identities,
                    super::capture::owner_reset::BufferIdentities {
                        targets: [
                            state.resources.headers.id(),
                            state.resources.samples.id(),
                            state.resources.frame.id(),
                        ],
                        staging,
                    },
                )
            })();
            if let Err(error) = result {
                error!("{error}");
                state.fail(MistGpuError::CaptureAuditFailed);
            }
        }
    }
    if state.failed {
        for phase in phases.values_mut() {
            phase
                .items
                .retain(|item| identities.get(item.entity()).is_err());
        }
    }
}

fn acknowledge_submission(state: Option<ResMut<'_, UploadState>>) {
    let Some(mut state) = state else { return };
    let started = state.timing_enabled.then(Instant::now);
    acknowledge_submission_inner(&mut state);
    if let Err(error) = state.record_timing(TransferStage::Acknowledgment, started) {
        state.fail(error);
    }
}

fn acknowledge_submission_inner(state: &mut UploadState) {
    if state.failed {
        return;
    }
    let Some(index) = state.selected else {
        return;
    };
    if !state.copy_recorded.load(Ordering::Acquire) {
        state.fail(MistGpuError::MissingUpload);
        return;
    }
    let result = (|| -> Result<(), MistGpuError> {
        let mut publication = state
            .resources
            .publication
            .lock()
            .map_err(|_| MistGpuError::PublicationPoisoned)?;
        publication.sequence = state.pending.sequence;
        publication.slots.copy_from_slice(&state.pending.slots);
        publication.accounting.submitted_packets = publication
            .accounting
            .submitted_packets
            .checked_add(1)
            .ok_or(MistGpuError::CounterExhausted("submitted packet"))?;
        publication.accounting.submitted_bytes = publication
            .accounting
            .submitted_bytes
            .checked_add(state.resources.layout.total_bytes)
            .ok_or(MistGpuError::CounterExhausted("submitted byte"))?;
        Ok(())
    })();
    if let Err(error) = result {
        state.fail(error);
        return;
    }
    let UploadState {
        pending, published, ..
    } = state;
    published.copy_from_slice(&pending.slots);
    let slot = &state.staging[index];
    slot.state.store(AWAITING_MAP, Ordering::Release);
    let status = Arc::clone(&slot.state);
    let callback_gate = slot.callback_gate.clone();
    #[cfg(test)]
    let release_callback = Arc::clone(&slot.release_callback);
    slot.buffer
        .slice(..)
        .map_async(MapMode::Write, move |result| {
            let next = if result.is_err() {
                MAP_FAILED
            } else if callback_gate
                .as_ref()
                .is_some_and(|gate| !gate.load(Ordering::Acquire))
            {
                HELD_MAP
            } else {
                WRITABLE
            };
            #[cfg(test)]
            let next = if next == WRITABLE && !release_callback.load(Ordering::Acquire) {
                // A real map has finished; the test controls when CPU readiness is delivered.
                HELD_MAP
            } else {
                next
            };
            status.store(next, Ordering::Release);
        });
    state.selected = None;
}

// The production guard runs earlier; this opt-in observer verifies the resulting
// actual phase after every material queue, including the owning bubble shell.
fn inspect_failed_light_phase(
    gate: Option<Res<'_, super::callback_gate::CallbackGate>>,
    admission: Res<'_, LightAdmission>,
    state: Option<Res<'_, UploadState>>,
    phases: Res<'_, ViewSortedRenderPhases<Transparent3d>>,
    identities: Query<'_, '_, &MistGpuIdentity>,
    shells: Option<Res<'_, bevy::pbr::RenderMaterialInstances<crate::BubbleDomeMaterial>>>,
) {
    let (Some(gate), LightAdmission::Failed(error), Some(state)) = (gate, *admission, state) else {
        return;
    };
    let result = (|| -> Result<(), super::callback_gate::GateError> {
        let publication = state
            .resources
            .publication
            .lock()
            .map_err(|_| super::callback_gate::GateError::Poisoned)?;
        let mut mist = 0;
        let mut others = 0;
        let mut shell_entities = [0; 8];
        let mut shell_count = 0;
        for phase in phases.values() {
            for item in &phase.items {
                if identities.contains(item.entity()) {
                    mist += 1;
                } else {
                    others += 1;
                    if shells
                        .as_ref()
                        .is_some_and(|materials| materials.contains_key(&item.main_entity()))
                    {
                        let entry = shell_entities.get_mut(shell_count).ok_or(
                            super::callback_gate::GateError::Failure(
                                "shell evidence capacity exhausted",
                            ),
                        )?;
                        *entry = item.main_entity().id().to_bits();
                        shell_count += 1;
                    }
                }
            }
        }
        gate.record_failure(
            error,
            state.failed,
            publication.sequence,
            publication.accounting,
            phases.len(),
            mist,
            others,
            &shell_entities[..shell_count],
        )
    })();
    if let Err(error) = result {
        error!("Additional failed-light verification error: {error}");
    }
}
