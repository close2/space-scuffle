//! Opt-in asynchronous readback of the renderer's actual per-view light buffers.
//!
//! Capture requests are frozen by extraction before view preparation. Compute reads
//! the real dynamic uniform binding after preparation and calls the shipped lighting
//! function. Normal gameplay installs none of these diagnostic buffers or systems.

use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::pbr::{GpuLights, LightMeta, ViewLightsUniformOffset, MAX_DIRECTIONAL_LIGHTS};
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_phase::{PhaseItem, ViewSortedRenderPhases};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, BindingType, Buffer,
    BufferBindingType, BufferDescriptor, BufferInitDescriptor, BufferSize, BufferUsages,
    CachedComputePipelineId, CachedPipelineState, CommandEncoderDescriptor, ComputePassDescriptor,
    ComputePipeline, ComputePipelineDescriptor, MapMode, Pipeline, PipelineCache,
    PipelineCacheError, ShaderDefVal, ShaderStages, ShaderType,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::MainEntity;
use bevy::render::{Render, RenderApp, RenderSet};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A complete main-world diagnostic request, copied at extraction before rendering.
#[derive(Resource, Clone, Copy, Default, ExtractResource)]
pub(super) struct ProbeRequest {
    /// Monotonic capture condition identifier; zero means no request.
    pub nonce: u64,
    /// Main-world camera identities, at most two for the bounded fixture.
    pub views: [Option<Entity>; 2],
}

/// Exact output layout used by the diagnostic compute entrypoint.
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct LightReading {
    /// Direct RGB response for world X, Y, and Z normals at the authored reference.
    pub axes: [[f32; 4]; 3],
    /// Total prepared directional count, eligible count, and reserved lanes.
    pub counts: [u32; 4],
    /// Actual packed linear RGB-times-lux for each prepared light.
    pub colors: [[f32; 4]; MAX_DIRECTIONAL_LIGHTS],
    /// Actual world-space direction to light and per-view skip bit.
    pub directions: [[f32; 4]; MAX_DIRECTIONAL_LIGHTS],
}

/// One actual prepared view's buffer address and computed lighting response.
#[derive(Clone, Serialize)]
pub(super) struct ViewReading {
    /// Main-world camera identity from the extraction request.
    pub camera: u64,
    /// Actual dynamic uniform byte offset, supplied exactly once at binding.
    pub offset: u32,
    /// Actual buffer identity for this prepared frame.
    pub buffer: String,
    /// Actual buffer allocation size in bytes.
    pub buffer_bytes: u64,
    /// Actual queued mist items for this prepared camera after admission and sorting.
    pub queued_mist: usize,
    /// GPU-computed diagnostics from that buffer and offset.
    pub reading: LightReading,
}

/// Bounded result correlated with the extracted main-world request.
#[derive(Clone, Serialize)]
pub(super) struct ProbeReport {
    /// Main-world capture condition identifier.
    pub nonce: u64,
    /// Actual view observations, in requested camera order.
    pub views: Vec<ViewReading>,
}

/// Diagnostic failures are sticky and make capture completion fail explicitly.
#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum ProbeError {
    /// Shared readback ownership was poisoned.
    #[error("view-light probe mutex poisoned")]
    Poisoned,
    /// Required real shader, view, binding, or readback violated its contract.
    #[error("view-light probe contract failed: {0}")]
    Contract(&'static str),
    /// The actual specialized compute shader failed.
    #[error("view-light probe shader failed: {0}")]
    Shader(String),
    /// Actual device mapping failed.
    #[error("view-light probe GPU mapping failed: {0}")]
    Mapping(String),
}

/// Shared completed evidence; populated only after a real GPU mapping callback.
#[derive(Resource, Clone, Default)]
pub(super) struct ProbeEvidence(Arc<Mutex<Option<Result<ProbeReport, ProbeError>>>>);

impl ProbeEvidence {
    /// Reads the newest completed result, propagating any sticky diagnostic failure.
    pub(super) fn read(&self, nonce: u64) -> Result<Option<ProbeReport>, ProbeError> {
        let value = self.0.lock().map_err(|_| ProbeError::Poisoned)?;
        match value.as_ref() {
            Some(Ok(report)) if report.nonce == nonce => Ok(Some(report.clone())),
            Some(Err(error)) => Err(error.clone()),
            _ => Ok(None),
        }
    }

    fn publish(&self, value: Result<ProbeReport, ProbeError>) {
        if let Ok(mut output) = self.0.lock() {
            if !matches!(*output, Some(Err(_))) {
                *output = Some(value);
            }
        } else {
            error!("{}", ProbeError::Poisoned);
        }
    }
}

#[derive(Resource)]
struct ProbeShader(Handle<Shader>);

struct Pending {
    report: ProbeReport,
    complete: Arc<Mutex<Option<Result<(), String>>>>,
}

#[derive(Resource)]
struct ProbeGpu {
    pipeline: CachedComputePipelineId,
    lights_layout: BindGroupLayout,
    result_group: BindGroup,
    output: Buffer,
    readback: Buffer,
    pending: Option<Pending>,
    submitted_nonce: u64,
}

const ENTRY: &str = r"
struct LightReading {
    axes: array<vec4<f32>, 3>, counts: vec4<u32>,
    colors: array<vec4<f32>, #{MAX_DIRECTIONAL_LIGHTS}>,
    directions: array<vec4<f32>, #{MAX_DIRECTIONAL_LIGHTS}>,
};
@group(1) @binding(0) var<storage, read_write> result: LightReading;
@group(1) @binding(1) var<uniform> reference: vec4<f32>;
@compute @workgroup_size(1)
fn read_lights() {
    result.axes[0] = vec4<f32>(mist_direct_light(vec3<f32>(1.0, 0.0, 0.0), reference.x), 0.0);
    result.axes[1] = vec4<f32>(mist_direct_light(vec3<f32>(0.0, 1.0, 0.0), reference.x), 0.0);
    result.axes[2] = vec4<f32>(mist_direct_light(vec3<f32>(0.0, 0.0, 1.0), reference.x), 0.0);
    var eligible = 0u;
    for (var i = 0u; i < lights.n_directional_lights; i += 1u) {
        let light = lights.directional_lights[i];
        if light.skip == 0u { eligible += 1u; }
        result.colors[i] = light.color;
        result.directions[i] = vec4<f32>(light.direction_to_light, f32(light.skip));
    }
    result.counts = vec4<u32>(lights.n_directional_lights, eligible, 0u, 0u);
}
";

/// Installs the capture-only probe using the same shader path and immutable style reference.
pub(super) fn install(
    app: &mut App,
    root: &Path,
    reference: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(root.join("shaders/mist_wake.wgsl"))?;
    let start = source
        .find("fn mist_direct_light(")
        .ok_or(ProbeError::Contract("shipped function missing"))?;
    let end = source[start..]
        .find("// Convert straight")
        .ok_or(ProbeError::Contract("shipped function boundary missing"))?
        + start;
    let shader = app
        .world_mut()
        .resource_mut::<Assets<Shader>>()
        .add(Shader::from_wgsl(
            format!(
                "#import bevy_pbr::mesh_view_bindings::lights\n{}\n{ENTRY}",
                &source[start..end]
            ),
            "shipped-mist-light-capture-probe.wgsl",
        ));
    let evidence = ProbeEvidence::default();
    app.insert_resource(evidence.clone())
        .init_resource::<ProbeRequest>()
        .add_plugins(ExtractResourcePlugin::<ProbeRequest>::default());
    app.sub_app_mut(RenderApp)
        .insert_resource(evidence)
        .insert_resource(ProbeShader(shader))
        .insert_resource(ProbeReference(reference))
        .add_systems(Render, update_probe.in_set(RenderSet::PrepareBindGroups));
    Ok(())
}

#[derive(Resource)]
struct ProbeReference(f32);

fn create(world: &World) -> Result<ProbeGpu, ProbeError> {
    let device = world.resource::<RenderDevice>();
    let lights_layout = device.create_bind_group_layout(
        "actual prepared lights capture",
        &[BindGroupLayoutEntry {
            binding: 1,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: Some(GpuLights::min_size()),
            },
            count: None,
        }],
    );
    let result_layout = device.create_bind_group_layout(
        "light capture output",
        &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(16),
                },
                count: None,
            },
        ],
    );
    let size = u64::try_from(size_of::<LightReading>())
        .map_err(|_| ProbeError::Contract("reading size overflow"))?;
    let output = device.create_buffer(&BufferDescriptor {
        label: Some("light capture output"),
        size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("two-view light capture readback"),
        size: size * 2,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let reference = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("immutable admitted mist reference"),
        contents: bytemuck::cast_slice(&[world.resource::<ProbeReference>().0, 0.0, 0.0, 0.0]),
        usage: BufferUsages::UNIFORM,
    });
    let result_group = device.create_bind_group(
        "light capture output",
        &result_layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: output.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: reference.as_entire_binding(),
            },
        ],
    );
    let pipeline =
        world
            .resource::<PipelineCache>()
            .queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("actual mist per-view lighting capture".into()),
                layout: vec![lights_layout.clone(), result_layout],
                push_constant_ranges: vec![],
                shader: world.resource::<ProbeShader>().0.clone(),
                shader_defs: vec![ShaderDefVal::UInt(
                    "MAX_DIRECTIONAL_LIGHTS".into(),
                    u32::try_from(MAX_DIRECTIONAL_LIGHTS)
                        .map_err(|_| ProbeError::Contract("light limit overflow"))?,
                )],
                entry_point: "read_lights".into(),
                zero_initialize_workgroup_memory: true,
            });
    Ok(ProbeGpu {
        pipeline,
        lights_layout,
        result_group,
        output,
        readback,
        pending: None,
        submitted_nonce: 0,
    })
}

fn update_probe(world: &mut World) {
    if !world.contains_resource::<ProbeGpu>() {
        match create(world) {
            Ok(state) => {
                world.insert_resource(state);
            }
            Err(error) => {
                world.resource::<ProbeEvidence>().publish(Err(error));
                return;
            }
        }
    }
    let result =
        world.resource_scope(|world, mut state: Mut<'_, ProbeGpu>| update(world, &mut state));
    if let Err(error) = result {
        world.resource::<ProbeEvidence>().publish(Err(error));
    }
}

fn prepared_pipeline(
    state: Option<&CachedPipelineState>,
) -> Result<Option<ComputePipeline>, ProbeError> {
    match state {
        None
        | Some(
            CachedPipelineState::Queued
            | CachedPipelineState::Creating(_)
            | CachedPipelineState::Err(
                PipelineCacheError::ShaderNotLoaded(_)
                | PipelineCacheError::ShaderImportNotYetAvailable,
            ),
        ) => Ok(None),
        Some(CachedPipelineState::Err(error)) => Err(ProbeError::Shader(error.to_string())),
        Some(CachedPipelineState::Ok(Pipeline::ComputePipeline(pipeline))) => {
            Ok(Some(pipeline.clone()))
        }
        Some(CachedPipelineState::Ok(Pipeline::RenderPipeline(_))) => Err(ProbeError::Contract(
            "diagnostic compute ID resolved to render pipeline",
        )),
    }
}

fn update(world: &mut World, state: &mut ProbeGpu) -> Result<(), ProbeError> {
    if let Some(pending) = &state.pending {
        let completed = pending
            .complete
            .lock()
            .map_err(|_| ProbeError::Poisoned)?
            .take();
        let Some(completed) = completed else {
            return Ok(());
        };
        completed.map_err(ProbeError::Mapping)?;
        let mut report = pending.report.clone();
        {
            let mapped = state.readback.slice(..).get_mapped_range();
            let readings: &[LightReading] = bytemuck::cast_slice(&mapped);
            for (view, reading) in report.views.iter_mut().zip(readings) {
                view.reading = *reading;
            }
        }
        state.readback.unmap();
        state.pending = None;
        world.resource::<ProbeEvidence>().publish(Ok(report));
    }
    let Some(request) = world.get_resource::<ProbeRequest>().copied() else {
        return Ok(());
    };
    if request.nonce == 0 || request.nonce <= state.submitted_nonce {
        return Ok(());
    }
    let cache = world.resource::<PipelineCache>();
    // Queueing reserves an ID before RenderSet::Render publishes its cache entry.
    // An immediate first request must not call the cache's indexing getters yet.
    let published = cache.pipelines().nth(state.pipeline.id());
    let Some(pipeline) = prepared_pipeline(published.map(|entry| &entry.state))? else {
        return Ok(());
    };
    let mut query = world.query::<(Entity, MainEntity, &ViewLightsUniformOffset)>();
    let offsets = request.views.map(|view| {
        view.and_then(|view| {
            query
                .iter(world)
                .find(|(_, main, _)| *main == view)
                .map(|(render_entity, _, offset)| (view, render_entity, offset.offset))
        })
    });
    if request.views.iter().flatten().count() != offsets.iter().flatten().count() {
        return Ok(());
    }
    let device = world.resource::<RenderDevice>();
    let queue = world.resource::<RenderQueue>();
    let lights = world.resource::<LightMeta>();
    let Some(buffer) = lights.view_gpu_lights.buffer() else {
        return Ok(());
    };
    let Some(binding) = lights.view_gpu_lights.binding() else {
        return Ok(());
    };
    let group = device.create_bind_group(
        "actual view light buffer capture",
        &state.lights_layout,
        &[BindGroupEntry {
            binding: 1,
            resource: binding,
        }],
    );
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("two actual view light reads"),
    });
    let mut views = Vec::with_capacity(2);
    let size = u64::try_from(size_of::<LightReading>())
        .map_err(|_| ProbeError::Contract("reading size overflow"))?;
    for (index, (camera, render_entity, offset)) in offsets.into_iter().flatten().enumerate() {
        if offset % device.limits().min_uniform_buffer_offset_alignment != 0
            || u64::from(offset) + GpuLights::min_size().get() > buffer.size()
        {
            return Err(ProbeError::Contract(
                "actual dynamic uniform range/alignment invalid",
            ));
        }
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("actual per-view mist lighting"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &*group, &[offset]);
            pass.set_bind_group(1, &*state.result_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &state.output,
            0,
            &state.readback,
            u64::try_from(index).map_err(|_| ProbeError::Contract("view index overflow"))? * size,
            size,
        );
        views.push(ViewReading {
            camera: camera.to_bits(),
            offset,
            buffer: format!("{:?}", buffer.id()),
            buffer_bytes: buffer.size(),
            queued_mist: world
                .get_resource::<ViewSortedRenderPhases<Transparent3d>>()
                .and_then(|phases| phases.get(&render_entity))
                .map_or(0, |phase| {
                    phase
                        .items
                        .iter()
                        .filter(|item| {
                            world
                                .get::<super::upload::MistGpuIdentity>(item.entity())
                                .is_some()
                        })
                        .count()
                }),
            reading: bytemuck::Zeroable::zeroed(),
        });
    }
    if views.is_empty() {
        return Err(ProbeError::Contract("capture request has no camera"));
    }
    queue.submit([encoder.finish()]);
    let complete = Arc::new(Mutex::new(None));
    let callback = Arc::clone(&complete);
    state
        .readback
        .slice(..)
        .map_async(MapMode::Read, move |result| {
            if let Ok(mut value) = callback.lock() {
                *value = Some(result.map_err(|error| error.to_string()));
            }
        });
    state.submitted_nonce = request.nonce;
    state.pending = Some(Pending {
        report: ProbeReport {
            nonce: request.nonce,
            views,
        },
        complete,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpublished_and_retryable_pipeline_states_wait_without_indexing() -> Result<(), ProbeError> {
        assert!(prepared_pipeline(None)?.is_none());
        assert!(prepared_pipeline(Some(&CachedPipelineState::Queued))?.is_none());
        for error in [
            PipelineCacheError::ShaderNotLoaded(Handle::<Shader>::default().id()),
            PipelineCacheError::ShaderImportNotYetAvailable,
        ] {
            assert!(prepared_pipeline(Some(&CachedPipelineState::Err(error)))?.is_none());
        }
        Ok(())
    }

    #[test]
    fn real_compiler_failure_is_not_reclassified_as_pending() {
        let state = CachedPipelineState::Err(PipelineCacheError::CreateShaderModule(
            "invalid actual shader module".into(),
        ));
        assert!(
            matches!(prepared_pipeline(Some(&state)), Err(ProbeError::Shader(message)) if message.contains("invalid actual shader module"))
        );
    }
}
