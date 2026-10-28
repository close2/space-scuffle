//! Bounded mist packets and publication metadata shared by the main and render worlds.
//!
//! World-space samples, relative ages and frame time form one indivisible packet. The
//! main world never exposes a reused slot until the renderer acknowledges its generation.

use super::{emission::EmitterHistory, material::MistWakeUniformGpu, MistStyle};
use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::RenderDevice;
use std::sync::{Arc, Mutex};

/// Explicit failures invalidate mist rendering instead of publishing partial geometry.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MistGpuError {
    /// Current authored view-light inputs failed admission; retained packets cannot draw.
    #[error(transparent)]
    LightAdmission(#[from] super::MistLightError),
    /// Optional real-render owner lifecycle audit rejected a queued packet.
    #[error("mist owner-reset queue verification failed; see original audit diagnostic")]
    CaptureAuditFailed,
    /// Capacity arithmetic or a packed index cannot be represented.
    #[error("mist GPU capacity/layout arithmetic is invalid")]
    InvalidLayout,
    /// The actual adapter cannot bind the configured storage budget.
    #[error("mist GPU adapter limit exceeded: {0}")]
    AdapterLimit(&'static str),
    /// A nonfinite value or inconsistent chronological sample reached a packet.
    #[error("mist GPU packet contains invalid position, age, radius, or frame data")]
    InvalidPacket,
    /// A fixed ownership or publication counter exhausted its representation.
    #[error("mist GPU {0} counter exhausted")]
    CounterExhausted(&'static str),
    /// Cross-world publication ownership was poisoned by a prior panic.
    #[error("mist GPU publication mutex is poisoned")]
    PublicationPoisoned,
    /// The backend could not return an existing staging buffer to CPU ownership.
    #[error("mist GPU staging buffer {0} remapping failed")]
    MappingFailed(usize),
    /// Backpressure exceeded its bounded readiness interval.
    #[error("mist GPU staging backpressure exceeded {0} wall seconds")]
    StagingTimeout(f32),
    /// Queueing found a generation different from the selected packet.
    #[error("mist GPU stale generation queued in slot {slot}: draw {draw}, packet {packet}")]
    StaleGeneration {
        /// Explicit emitter-slot index.
        slot: u32,
        /// Extracted draw owner's generation.
        draw: u32,
        /// Generation in the complete packet selected before queueing.
        packet: u32,
    },
    /// A queued identity is outside the validated slot capacity.
    #[error("mist GPU queued slot {0} is outside its configured capacity")]
    InvalidQueuedSlot(u32),
    /// A queued mist draw has no corresponding extracted camera.
    #[error("mist GPU transparent phase is missing its extracted view")]
    MissingQueuedView,
    /// The pre-camera graph node did not record the selected copy.
    #[error("mist GPU selected packet was not recorded before render submission")]
    MissingUpload,
    /// Bounded measurement could not record a finite duration or exhausted capacity.
    #[error("mist GPU bounded transfer measurement failed")]
    MeasurementFailed,
}

/// Four aligned lanes consumed by the production vertex shader (64 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EmitterHeaderGpu {
    /// Sample base, live count, ownership generation, enabled bit.
    pub control: UVec4,
    /// World-space sorting origin and radius in world units.
    pub origin_radius: Vec4,
    /// World-space conveyor translation and overclock accent bit as 0/1.
    pub offset_boost: Vec4,
    /// Sample capacity followed by initialized reserved integer lanes.
    pub capacity: UVec4,
}

/// World-space sample plus relative age and coherent path distance (32 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SampleGpu {
    /// World position XYZ and age at packet extraction, in seconds.
    pub position_age: Vec4,
    /// Path distance in world units followed by initialized reserved lanes.
    pub distance: Vec4,
}

/// Atomically published art/time and geometry parameters (80 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameGpu {
    /// Existing linear-color/light/time contract, unchanged by this storage refactor.
    pub appearance: MistWakeUniformGpu,
    /// Lifetime seconds, width/radius ratio, and two initialized reserved lanes.
    pub geometry: Vec4,
}

impl FrameGpu {
    pub(super) fn validate(&self) -> Result<(), MistGpuError> {
        if !self.appearance.cool.is_finite()
            || !self.appearance.warm.is_finite()
            || !self.appearance.boost.is_finite()
            || !self.appearance.light.is_finite()
            || !self.geometry.is_finite()
            || self.geometry.x <= 0.0
            || self.geometry.y <= 0.0
        {
            return Err(MistGpuError::InvalidPacket);
        }
        Ok(())
    }
}

/// Checked offsets and capacities for a complete bounded transfer packet.
#[derive(Debug, Clone, Copy)]
pub struct PacketLayout {
    /// Number of independently owned emitter slots.
    pub emitters: usize,
    /// Chronological sample capacity per slot.
    pub samples_per_emitter: usize,
    /// Byte size of the emitter-header target buffer.
    pub header_bytes: u64,
    /// Byte size of the history-sample target buffer.
    pub sample_bytes: u64,
    /// Byte offset of frame data within each mapped staging packet.
    pub frame_offset: u64,
    /// Complete mapped staging-packet size, including its shared frame.
    pub total_bytes: u64,
}

impl PacketLayout {
    /// Checks capacities and all byte/index arithmetic before any CPU/GPU allocation.
    ///
    /// # Errors
    /// Returns [`MistGpuError::InvalidLayout`] for invalid or unrepresentable capacities.
    pub fn new(emitters: usize, samples_per_emitter: usize) -> Result<Self, MistGpuError> {
        if !(1..=4096).contains(&emitters) || !(4..=128).contains(&samples_per_emitter) {
            return Err(MistGpuError::InvalidLayout);
        }
        let samples = emitters
            .checked_mul(samples_per_emitter)
            .ok_or(MistGpuError::InvalidLayout)?;
        u32::try_from(samples).map_err(|_| MistGpuError::InvalidLayout)?;
        let header_bytes = u64::try_from(emitters)
            .ok()
            .and_then(|n| n.checked_mul(size_of::<EmitterHeaderGpu>() as u64))
            .ok_or(MistGpuError::InvalidLayout)?;
        let sample_bytes = u64::try_from(samples)
            .ok()
            .and_then(|n| n.checked_mul(size_of::<SampleGpu>() as u64))
            .ok_or(MistGpuError::InvalidLayout)?;
        let frame_offset = header_bytes
            .checked_add(sample_bytes)
            .ok_or(MistGpuError::InvalidLayout)?;
        let total_bytes = frame_offset
            .checked_add(size_of::<FrameGpu>() as u64)
            .ok_or(MistGpuError::InvalidLayout)?;
        Ok(Self {
            emitters,
            samples_per_emitter,
            header_bytes,
            sample_bytes,
            frame_offset,
            total_bytes,
        })
    }

    /// Validates real adapter storage/buffer limits before creating persistent buffers.
    ///
    /// # Errors
    /// Returns a named [`MistGpuError::AdapterLimit`] for an unsupported binding budget.
    pub fn validate_device(&self, device: &RenderDevice) -> Result<(), MistGpuError> {
        let limits = device.limits();
        self.validate_limits(
            u64::from(limits.max_storage_buffer_binding_size),
            limits.max_buffer_size,
            limits.max_storage_buffers_per_shader_stage,
        )
    }

    pub(super) fn validate_limits(
        &self,
        max_binding: u64,
        max_buffer: u64,
        storage_bindings: u32,
    ) -> Result<(), MistGpuError> {
        if self.header_bytes.max(self.sample_bytes) > max_binding {
            return Err(MistGpuError::AdapterLimit("storage binding bytes"));
        }
        if self.total_bytes > max_buffer {
            return Err(MistGpuError::AdapterLimit("staging buffer bytes"));
        }
        if storage_bindings < 3 {
            return Err(MistGpuError::AdapterLimit(
                "three storage bindings per stage",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PublishedSlot {
    pub generation: u32,
    pub count: u32,
    pub origin: Vec3,
    pub minimum: Vec3,
    pub maximum: Vec3,
}

/// Application-owned buffer accounting; these are not driver allocation counts.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub(super) struct GpuAccounting {
    pub target_buffers: u64,
    pub target_bytes: u64,
    pub staging_buffers: u64,
    pub staging_bytes: u64,
    pub submitted_packets: u64,
    pub submitted_bytes: u64,
    pub staging_stalls: u64,
}

impl GpuAccounting {
    pub fn checked_delta(self, previous: Self) -> Result<Self, MistGpuError> {
        let subtract = |current: u64, prior: u64| {
            current
                .checked_sub(prior)
                .ok_or(MistGpuError::CounterExhausted("regressed accounting"))
        };
        Ok(Self {
            target_buffers: subtract(self.target_buffers, previous.target_buffers)?,
            target_bytes: subtract(self.target_bytes, previous.target_bytes)?,
            staging_buffers: subtract(self.staging_buffers, previous.staging_buffers)?,
            staging_bytes: subtract(self.staging_bytes, previous.staging_bytes)?,
            submitted_packets: subtract(self.submitted_packets, previous.submitted_packets)?,
            submitted_bytes: subtract(self.submitted_bytes, previous.submitted_bytes)?,
            staging_stalls: subtract(self.staging_stalls, previous.staging_stalls)?,
        })
    }
}

pub(super) struct Publication {
    pub sequence: u64,
    pub slots: Vec<PublishedSlot>,
    pub error: Option<MistGpuError>,
    pub accounting: GpuAccounting,
    pub measurements: Option<super::benchmark::TransferMeasurements>,
}

impl Publication {
    pub fn new(layout: PacketLayout) -> Self {
        Self {
            sequence: 0,
            slots: vec![PublishedSlot::default(); layout.emitters],
            error: None,
            accounting: GpuAccounting::default(),
            measurements: None,
        }
    }
}

#[derive(Resource, Clone)]
pub(super) struct GpuResources {
    pub layout: PacketLayout,
    pub headers: Buffer,
    pub samples: Buffer,
    pub frame: Buffer,
    pub publication: Arc<Mutex<Publication>>,
}

impl GpuResources {
    pub fn new(
        device: &RenderDevice,
        layout: PacketLayout,
        publication: Arc<Mutex<Publication>>,
    ) -> Result<Self, MistGpuError> {
        layout.validate_device(device)?;
        let create = |label, size| {
            device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        let resources = Self {
            layout,
            headers: create("mist persistent headers", layout.header_bytes),
            samples: create("mist persistent samples", layout.sample_bytes),
            frame: create("mist persistent frame", size_of::<FrameGpu>() as u64),
            publication,
        };
        {
            let mut published = resources
                .publication
                .lock()
                .map_err(|_| MistGpuError::PublicationPoisoned)?;
            published.accounting.target_buffers = published
                .accounting
                .target_buffers
                .checked_add(3)
                .ok_or(MistGpuError::CounterExhausted("target buffer"))?;
            published.accounting.target_bytes = published
                .accounting
                .target_bytes
                .checked_add(layout.total_bytes)
                .ok_or(MistGpuError::CounterExhausted("target bytes"))?;
        }
        Ok(resources)
    }
}

pub(super) struct Packet {
    pub headers: Vec<EmitterHeaderGpu>,
    pub samples: Vec<SampleGpu>,
    pub frame: FrameGpu,
    pub slots: Vec<PublishedSlot>,
    pub sequence: u64,
}

impl Packet {
    pub fn new(layout: PacketLayout) -> Self {
        Self {
            headers: vec![EmitterHeaderGpu::default(); layout.emitters],
            samples: vec![SampleGpu::default(); layout.emitters * layout.samples_per_emitter],
            frame: FrameGpu::default(),
            slots: vec![PublishedSlot::default(); layout.emitters],
            sequence: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn write_slot(
        &mut self,
        index: usize,
        layout: PacketLayout,
        history: &EmitterHistory,
        generation: u32,
        origin: Vec3,
        offset: Vec3,
        radius: f32,
        boosted: bool,
        style: &MistStyle,
    ) -> Result<(), MistGpuError> {
        if index >= layout.emitters
            || history.len() > layout.samples_per_emitter
            || !origin.is_finite()
            || !offset.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
        {
            return Err(MistGpuError::InvalidPacket);
        }
        let base = index * layout.samples_per_emitter;
        let count = history.len();
        let mut minimum = origin;
        let mut maximum = origin;
        for knot in 0..layout.samples_per_emitter {
            self.samples[base + knot] = if let Some(sample) = history.sample(knot) {
                let age = (history.clock() - sample.birth) as f32;
                if !sample.position.is_finite()
                    || !age.is_finite()
                    || age < 0.0
                    || !sample.distance.is_finite()
                {
                    return Err(MistGpuError::InvalidPacket);
                }
                let center = sample.position + offset;
                if !center.is_finite() {
                    return Err(MistGpuError::InvalidPacket);
                }
                minimum = minimum.min(center);
                maximum = maximum.max(center);
                SampleGpu {
                    position_age: sample.position.extend(age),
                    distance: Vec4::new(sample.distance, 0.0, 0.0, 0.0),
                }
            } else {
                SampleGpu::default()
            };
        }
        let excursion = Vec3::splat(super::ribbon::maximum_sample_excursion(radius, style));
        if !(minimum - excursion).is_finite() || !(maximum + excursion).is_finite() {
            return Err(MistGpuError::InvalidPacket);
        }
        self.headers[index] = EmitterHeaderGpu {
            control: UVec4::new(base as u32, count as u32, generation, u32::from(count > 1)),
            origin_radius: origin.extend(radius),
            offset_boost: offset.extend(f32::from(boosted)),
            capacity: UVec4::new(layout.samples_per_emitter as u32, 0, 0, 0),
        };
        self.slots[index] = PublishedSlot {
            generation,
            count: count as u32,
            origin,
            minimum: minimum - excursion,
            maximum: maximum + excursion,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn frame_and_accounting_reject_invalid_evidence() -> Result<(), MistGpuError> {
        let frame = FrameGpu {
            appearance: MistWakeUniformGpu::default(),
            geometry: Vec4::new(1.4, 0.7, 0.0, 0.0),
        };
        frame.validate()?;
        for lane in 0..20 {
            let mut invalid = frame;
            bytemuck::cast_slice_mut::<FrameGpu, f32>(std::slice::from_mut(&mut invalid))[lane] =
                f32::NAN;
            assert!(matches!(
                invalid.validate(),
                Err(MistGpuError::InvalidPacket)
            ));
        }
        for invalid_lifetime in [0.0, -1.0, f32::INFINITY] {
            let invalid = FrameGpu {
                geometry: Vec4::new(invalid_lifetime, 0.7, 0.0, 0.0),
                ..frame
            };
            assert!(invalid.validate().is_err());
        }
        let before = GpuAccounting {
            target_buffers: 3,
            staging_buffers: 3,
            submitted_packets: 12,
            submitted_bytes: 12 * 819_280,
            ..default()
        };
        let after = GpuAccounting {
            submitted_packets: 15,
            submitted_bytes: 15 * 819_280,
            ..before
        };
        let delta = after.checked_delta(before)?;
        assert_eq!(delta.target_buffers, 0);
        assert_eq!(delta.staging_buffers, 0);
        assert_eq!(delta.submitted_packets, 3);
        assert_eq!(delta.submitted_bytes, 3 * 819_280);
        assert!(before.checked_delta(after).is_err());
        Ok(())
    }

    #[test]
    fn production_shader_structs_match_rust_packet_fields() -> Result<(), Box<dyn std::error::Error>>
    {
        let shader = include_str!("../../../../assets/shaders/mist_wake.wgsl");
        let start = shader
            .find("struct EmitterHeader {")
            .ok_or("header declaration absent")?;
        let end = shader[start..]
            .find("@group(2)")
            .ok_or("storage binding absent")?
            + start;
        let module = naga::front::wgsl::parse_str(&shader[start..end])?;
        for (name, bytes, expected) in [
            (
                "EmitterHeader",
                size_of::<EmitterHeaderGpu>(),
                vec![
                    ("control", offset_of!(EmitterHeaderGpu, control)),
                    ("origin_radius", offset_of!(EmitterHeaderGpu, origin_radius)),
                    ("offset_boost", offset_of!(EmitterHeaderGpu, offset_boost)),
                    ("capacity", offset_of!(EmitterHeaderGpu, capacity)),
                ],
            ),
            (
                "TrailSample",
                size_of::<SampleGpu>(),
                vec![
                    ("position_age", offset_of!(SampleGpu, position_age)),
                    ("distance", offset_of!(SampleGpu, distance)),
                ],
            ),
            (
                "MistWakeUniform",
                size_of::<FrameGpu>(),
                vec![
                    ("cool", offset_of!(MistWakeUniformGpu, cool)),
                    ("warm", offset_of!(MistWakeUniformGpu, warm)),
                    ("boost", offset_of!(MistWakeUniformGpu, boost)),
                    ("light", offset_of!(MistWakeUniformGpu, light)),
                    ("geometry", offset_of!(FrameGpu, geometry)),
                ],
            ),
        ] {
            let ty = module
                .types
                .iter()
                .find(|(_, ty)| ty.name.as_deref() == Some(name))
                .map(|(_, ty)| ty)
                .ok_or("GPU struct absent")?;
            let naga::TypeInner::Struct { members, span } = &ty.inner else {
                return Err("GPU type is not a struct".into());
            };
            assert_eq!(*span as usize, bytes, "{name} stride");
            assert_eq!(members.len(), expected.len(), "{name} member count");
            for (member, (field, offset)) in members.iter().zip(expected) {
                assert_eq!(member.name.as_deref(), Some(field), "{name} field order");
                assert_eq!(member.offset as usize, offset, "{name}.{field} offset");
            }
        }
        Ok(())
    }

    #[test]
    fn packed_offsets_and_checked_adapter_budget() -> Result<(), MistGpuError> {
        assert_eq!(size_of::<EmitterHeaderGpu>(), 64);
        assert_eq!(offset_of!(EmitterHeaderGpu, origin_radius), 16);
        assert_eq!(offset_of!(EmitterHeaderGpu, offset_boost), 32);
        assert_eq!(offset_of!(EmitterHeaderGpu, capacity), 48);
        assert_eq!(size_of::<SampleGpu>(), 32);
        assert_eq!(offset_of!(SampleGpu, distance), 16);
        assert_eq!(size_of::<FrameGpu>(), 80);
        assert_eq!(offset_of!(FrameGpu, geometry), 64);
        let layout = PacketLayout::new(512, 48)?;
        assert_eq!(layout.header_bytes, 32_768);
        assert_eq!(layout.sample_bytes, 786_432);
        assert_eq!(layout.total_bytes, 819_280);
        assert!(layout.validate_limits(786_431, u64::MAX, 3).is_err());
        assert!(layout.validate_limits(u64::MAX, 819_279, 3).is_err());
        assert!(layout.validate_limits(u64::MAX, u64::MAX, 2).is_err());
        layout.validate_limits(786_432, 819_280, 3)?;
        for invalid in [(0, 48), (512, 0), (usize::MAX, 128), (4096, 129)] {
            assert!(PacketLayout::new(invalid.0, invalid.1).is_err());
        }
        Ok(())
    }
}
