//! The minimap's renderer: one render-graph node, two pipelines of `assets/shaders/minimap.wgsl`,
//! and retained GPU resources written on change only (plan `2026-12-01-minimap` D-1).
//!
//! # Responsibility
//! - **Extraction.** [`extract_minimap`] mirrors the main world's
//!   [`MinimapFrame`](super::MinimapFrame) into [`ExtractedMinimap`] with `clone_from` into
//!   retained vectors, and only for the parts whose generation moved.
//! - **Uploads.** [`prepare_minimap_gpu`] writes the uniform every frame, the cell texture when
//!   the cells' generation moved, the fog texture when the fog's generation moved and the marks
//!   storage buffer when the marks' generation moved.
//!   A buffer or texture is created only when it is first needed or outgrown, and the bind group
//!   only when one of the resources it names was created. No Bevy asset is touched per frame.
//! - **Drawing.** [`MinimapNode`] is a top-level node with an edge to the camera driver, and so
//!   runs before egui's window pass, which `bevy_egui` adds after the camera driver
//!   (`render_systems.rs`, `setup_new_window_nodes_system`): egui samples the image this frame's
//!   pass wrote. It records one render pass into the minimap image: the full-screen terrain
//!   triangle, then one instanced draw of the marks.
//!
//! # Failure
//! A pipeline that fails to compile is an error, never a reason to draw something else (charter
//! §1.2, plan stop rule 6): [`report_minimap_pipelines`] logs it and marks
//! [`MinimapRenderEvidence`], and the main world's [`report_minimap_render_failure`] ends the app
//! with an error. A texture still loading is waited for, not a failure.
//!
//! # Invariants
//! - The node draws only when the target, the terrain's albedo and splat images and both
//!   pipelines are ready; otherwise it records nothing and the image keeps its last content.
//! - [`MinimapRenderEvidence`] counts every GPU resource created here, so tests can prove the
//!   steady state creates none (plan M-S3).

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::graph::CameraDriverLabel;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{
    Node, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel,
};
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only_sized, texture_2d, texture_2d_array, uniform_buffer_sized,
};
use bevy::render::render_resource::{
    AddressMode, BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BlendState,
    Buffer, BufferDescriptor, BufferUsages, CachedPipelineState, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, Extent3d, FilterMode, FragmentState, ImageCopyTexture,
    ImageDataLayout, LoadOp, MultisampleState, Operations, Origin3d, PipelineCache,
    PipelineCacheError, PrimitiveState, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
    StoreOp, Texture, TextureAspect, TextureDescriptor, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, VertexState,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSet};

use super::layout::MinimapLayout;
use super::style::{MinimapStyle, MinimapStyleStatus};
use super::{MinimapFrame, MinimapMark, MINIMAP_FORMAT};

/// The shader, beneath the assets root.
pub const MINIMAP_SHADER: &str = "shaders/minimap.wgsl";

/// Marks the storage buffer holds when first created; it doubles when outgrown.
const INITIAL_MARK_CAPACITY: usize = 256;

/// Format of the cell texture: one RGBA8 texel per map cell, read with `textureLoad`.
const CELL_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

/// Bytes per texel of the cell texture, as the copy layout counts them.
const CELL_TEXEL_BYTES: u32 = 4;
const _: () = assert!(CELL_TEXEL_BYTES as usize == super::terrain::CELL_TEXEL_BYTES);

/// Format of the fog texture: one byte per map cell, how far it darkens, sampled bilinearly.
const FOG_FORMAT: TextureFormat = TextureFormat::R8Unorm;

/// The shader's `MinimapUniform`, 112 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct MinimapUniform {
    background: [f32; 4],
    unpathable: [f32; 4],
    level_brightness: [f32; 4],
    border: [f32; 4],
    offset: [f32; 2],
    map_size: [f32; 2],
    scale: f32,
    side: f32,
    edge_mix: f32,
    terrain_on: u32,
    tick: u32,
    pad: [u32; 3],
}

/// What the renderer created and drew, shared between the main and render worlds.
///
/// Tests read it to prove that the steady state creates no GPU resource; the main world reads its
/// failure flag to end the app when a pipeline failed.
#[derive(Resource, Clone, Debug, Default)]
pub struct MinimapRenderEvidence(Arc<RenderCounters>);

/// The counters behind [`MinimapRenderEvidence`].
#[derive(Debug, Default)]
struct RenderCounters {
    failed: AtomicBool,
    frames: AtomicU64,
    uniform_buffers: AtomicU64,
    cell_textures: AtomicU64,
    fog_textures: AtomicU64,
    mark_buffers: AtomicU64,
    bind_groups: AtomicU64,
    drawn_marks: AtomicU64,
}

impl MinimapRenderEvidence {
    /// Whether a minimap pipeline failed to compile.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.0.failed.load(Ordering::Relaxed)
    }

    /// Frames on which the node recorded its pass.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.0.frames.load(Ordering::Relaxed)
    }

    /// GPU resources created so far: `[uniform buffers, cell textures, fog textures, mark
    /// buffers, bind groups]`.
    #[must_use]
    pub fn created(&self) -> [u64; 5] {
        [
            self.0.uniform_buffers.load(Ordering::Relaxed),
            self.0.cell_textures.load(Ordering::Relaxed),
            self.0.fog_textures.load(Ordering::Relaxed),
            self.0.mark_buffers.load(Ordering::Relaxed),
            self.0.bind_groups.load(Ordering::Relaxed),
        ]
    }

    /// Marks drawn by the latest pass.
    #[must_use]
    pub fn drawn_marks(&self) -> u64 {
        self.0.drawn_marks.load(Ordering::Relaxed)
    }
}

/// Installs the evidence in the main world, and extraction and preparation in the render world
/// when one exists (a renderer-less test app has none).
pub(super) fn build_render(app: &mut App) {
    let evidence = MinimapRenderEvidence::default();
    app.insert_resource(evidence.clone());
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render
        .insert_resource(evidence)
        .init_resource::<ExtractedMinimap>()
        .init_resource::<MinimapGpu>()
        .add_systems(ExtractSchedule, extract_minimap)
        .add_systems(
            Render,
            (
                report_minimap_pipelines.in_set(RenderSet::Queue),
                prepare_minimap_gpu.in_set(RenderSet::PrepareBindGroups),
            ),
        );
    let mut graph = render.world_mut().resource_mut::<RenderGraph>();
    graph.add_node(MinimapLabel, MinimapNode);
    graph.add_node_edge(MinimapLabel, CameraDriverLabel);
}

/// Creates the pipelines once the render device exists.
pub(super) fn finish_render(app: &mut App) {
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.init_resource::<MinimapPipeline>();
    }
}

/// The main world's frame as the render world last saw it.
#[derive(Resource, Debug, Default)]
pub struct ExtractedMinimap {
    image: Option<AssetId<Image>>,
    albedo: Option<AssetId<Image>>,
    splat: Option<AssetId<Image>>,
    layout: Option<MinimapLayout>,
    map_size: UVec2,
    cells: Vec<u8>,
    cells_generation: u64,
    fog: Vec<u8>,
    fog_generation: u64,
    marks: Vec<MinimapMark>,
    marks_generation: u64,
    terrain_on: bool,
    tick: u32,
    style: Option<MinimapStyle>,
}

/// Mirrors the frame, copying a vector only when its generation moved.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn extract_minimap(
    frame: Extract<'_, '_, Option<Res<'_, MinimapFrame>>>,
    style: Extract<'_, '_, Option<Res<'_, MinimapStyle>>>,
    mut extracted: ResMut<'_, ExtractedMinimap>,
) {
    let extracted = extracted.as_mut();
    let Some(frame) = frame.as_deref() else {
        extracted.image = None;
        return;
    };
    extracted.image = frame.image.as_ref().map(Handle::id);
    extracted.albedo = frame.albedo.as_ref().map(Handle::id);
    extracted.splat = frame.splat.as_ref().map(Handle::id);
    extracted.layout = frame.layout;
    extracted.map_size = frame.map_size;
    extracted.terrain_on = frame.terrain_on;
    extracted.tick = frame.tick;
    if extracted.cells_generation != frame.cells_generation {
        extracted.cells.clone_from(&frame.cells);
        extracted.cells_generation = frame.cells_generation;
    }
    if extracted.fog_generation != frame.fog_generation {
        extracted.fog.clone_from(&frame.fog);
        extracted.fog_generation = frame.fog_generation;
    }
    if extracted.marks_generation != frame.marks_generation {
        extracted.marks.clone_from(&frame.marks);
        extracted.marks_generation = frame.marks_generation;
    }
    match (style.as_deref(), extracted.style.as_mut()) {
        (Some(style), Some(held)) if held != style => held.clone_from(style),
        (Some(style), None) => extracted.style = Some(style.clone()),
        (None, _) => extracted.style = None,
        _ => {}
    }
}

/// The layout and pipelines of the shader.
#[derive(Resource)]
pub struct MinimapPipeline {
    layout: BindGroupLayout,
    terrain: CachedRenderPipelineId,
    marks: CachedRenderPipelineId,
}

impl FromWorld for MinimapPipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let layout = device.create_bind_group_layout(
            "minimap",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer_sized(
                        false,
                        NonZeroU64::new(size_of::<MinimapUniform>() as u64),
                    ),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_2d_array(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    storage_buffer_read_only_sized(false, None),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                ),
            ),
        );
        let shader = world.resource::<AssetServer>().load(MINIMAP_SHADER);
        let cache = world.resource::<PipelineCache>();
        let pipeline = |name: &'static str, vertex: &'static str, fragment: &'static str| {
            cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some(name.into()),
                layout: vec![layout.clone()],
                push_constant_ranges: Vec::new(),
                vertex: VertexState {
                    shader: shader.clone(),
                    shader_defs: Vec::new(),
                    entry_point: vertex.into(),
                    buffers: Vec::new(),
                },
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                fragment: Some(FragmentState {
                    shader: shader.clone(),
                    shader_defs: Vec::new(),
                    entry_point: fragment.into(),
                    targets: vec![Some(ColorTargetState {
                        format: MINIMAP_FORMAT,
                        blend: Some(BlendState::ALPHA_BLENDING),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                zero_initialize_workgroup_memory: false,
            })
        };
        let terrain = pipeline("minimap_terrain", "terrain_vs", "terrain_fs");
        let marks = pipeline("minimap_marks", "mark_vs", "mark_fs");
        Self {
            layout,
            terrain,
            marks,
        }
    }
}

/// Logs a minimap pipeline that failed to compile and marks the evidence, once.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn report_minimap_pipelines(
    cache: Res<'_, PipelineCache>,
    pipeline: Option<Res<'_, MinimapPipeline>>,
    evidence: Res<'_, MinimapRenderEvidence>,
) {
    let Some(pipeline) = pipeline else {
        return;
    };
    if evidence.failed() {
        return;
    }
    // A queued pipeline enters the cache's list only when the cache next processes its queue.
    let listed = cache.pipelines().count();
    if pipeline.terrain.id().max(pipeline.marks.id()) >= listed {
        return;
    }
    for id in [pipeline.terrain, pipeline.marks] {
        match cache.get_render_pipeline_state(id) {
            CachedPipelineState::Err(
                PipelineCacheError::ShaderNotLoaded(_)
                | PipelineCacheError::ShaderImportNotYetAvailable,
            ) => {}
            CachedPipelineState::Err(error) => {
                error!("minimap: the pipeline of {MINIMAP_SHADER} failed: {error}");
                evidence.0.failed.store(true, Ordering::Relaxed);
                return;
            }
            _ => {}
        }
    }
}

/// Ends the app with an error once the renderer reports a failed minimap pipeline, and shows the
/// failure in the HUD's banner.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn report_minimap_render_failure(
    evidence: Res<'_, MinimapRenderEvidence>,
    mut status: ResMut<'_, MinimapStyleStatus>,
    mut exit: EventWriter<'_, AppExit>,
    mut reported: Local<'_, bool>,
) {
    if *reported || !evidence.failed() {
        return;
    }
    *reported = true;
    status.reject_render();
    exit.send(AppExit::error());
}

/// The render world's retained GPU resources.
#[derive(Resource, Default)]
pub struct MinimapGpu {
    uniform: Option<Buffer>,
    sampler: Option<Sampler>,
    cells: Option<(Texture, TextureView, UVec2)>,
    cells_uploaded: u64,
    fog: Option<(Texture, TextureView, UVec2)>,
    fog_uploaded: u64,
    marks: Option<Buffer>,
    mark_capacity: usize,
    marks_uploaded: u64,
    mark_count: u32,
    bind_group: Option<BindGroup>,
    bound: Option<BoundResources>,
    target: Option<TextureView>,
}

/// The identities of what the bind group names, to rebuild it only when one changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BoundResources {
    cells: bevy::render::render_resource::TextureViewId,
    albedo: bevy::render::render_resource::TextureViewId,
    splat: bevy::render::render_resource::TextureViewId,
    marks: bevy::render::render_resource::BufferId,
    fog: bevy::render::render_resource::TextureViewId,
}

/// Writes the uniform, uploads what changed and keeps the bind group current.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn prepare_minimap_gpu(
    device: Res<'_, RenderDevice>,
    queue: Res<'_, RenderQueue>,
    images: Res<'_, RenderAssets<GpuImage>>,
    pipeline: Option<Res<'_, MinimapPipeline>>,
    extracted: Res<'_, ExtractedMinimap>,
    evidence: Res<'_, MinimapRenderEvidence>,
    mut gpu: ResMut<'_, MinimapGpu>,
) {
    let gpu = gpu.as_mut();
    gpu.target = None;
    let (Some(pipeline), Some(layout), Some(style)) =
        (pipeline, extracted.layout, extracted.style.as_ref())
    else {
        return;
    };
    let lookup = |id: Option<AssetId<Image>>| id.and_then(|id| images.get(id));
    let (Some(target), Some(albedo), Some(splat)) = (
        lookup(extracted.image),
        lookup(extracted.albedo),
        lookup(extracted.splat),
    ) else {
        return;
    };
    let counters = &evidence.0;
    write_uniform(&device, &queue, gpu, counters, &extracted, layout, style);
    upload_cells(&device, &queue, gpu, counters, &extracted);
    upload_fog(&device, &queue, gpu, counters, &extracted);
    upload_marks(&device, &queue, gpu, counters, &extracted);
    let (Some(uniform), Some((_, cells, _)), Some((_, fog, _)), Some(marks)) =
        (&gpu.uniform, &gpu.cells, &gpu.fog, &gpu.marks)
    else {
        return;
    };
    let bound = BoundResources {
        cells: cells.id(),
        albedo: albedo.texture_view.id(),
        splat: splat.texture_view.id(),
        marks: marks.id(),
        fog: fog.id(),
    };
    if gpu.bound != Some(bound) {
        let sampler = gpu.sampler.get_or_insert_with(|| {
            device.create_sampler(&SamplerDescriptor {
                label: Some("minimap_linear"),
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                mipmap_filter: FilterMode::Linear,
                ..default()
            })
        });
        gpu.bind_group = Some(device.create_bind_group(
            "minimap",
            &pipeline.layout,
            &BindGroupEntries::sequential((
                uniform.as_entire_binding(),
                cells,
                &splat.texture_view,
                &*sampler,
                &albedo.texture_view,
                &*sampler,
                marks.as_entire_binding(),
                fog,
            )),
        ));
        gpu.bound = Some(bound);
        counters.bind_groups.fetch_add(1, Ordering::Relaxed);
    }
    gpu.target = Some(target.texture_view.clone());
}

/// Writes this frame's uniform, creating its buffer once.
fn write_uniform(
    device: &RenderDevice,
    queue: &RenderQueue,
    gpu: &mut MinimapGpu,
    counters: &RenderCounters,
    extracted: &ExtractedMinimap,
    layout: MinimapLayout,
    style: &MinimapStyle,
) {
    let uniform = gpu.uniform.get_or_insert_with(|| {
        counters.uniform_buffers.fetch_add(1, Ordering::Relaxed);
        device.create_buffer(&BufferDescriptor {
            label: Some("minimap_uniform"),
            size: size_of::<MinimapUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    });
    let values = MinimapUniform {
        background: style.background.linear,
        unpathable: style.unpathable.linear,
        level_brightness: style.level_brightness,
        border: style.border.linear,
        offset: layout.offset.to_array(),
        map_size: [layout.map_width, layout.map_height],
        scale: layout.scale,
        side: layout.side,
        edge_mix: style.edge_mix,
        terrain_on: u32::from(extracted.terrain_on),
        tick: extracted.tick,
        pad: [0; 3],
    };
    queue.write_buffer(uniform, 0, bytemuck::bytes_of(&values));
}

/// Uploads the cell texels when their generation moved, creating the texture for a new map size.
fn upload_cells(
    device: &RenderDevice,
    queue: &RenderQueue,
    gpu: &mut MinimapGpu,
    counters: &RenderCounters,
    extracted: &ExtractedMinimap,
) {
    let size = extracted.map_size;
    let expected = size.x as usize * size.y as usize * super::terrain::CELL_TEXEL_BYTES;
    if size.x == 0 || size.y == 0 || extracted.cells.len() != expected {
        return;
    }
    if gpu.cells.as_ref().is_none_or(|(_, _, held)| *held != size) {
        gpu.cells = Some(cell_texture(device, "minimap_cells", CELL_FORMAT, size));
        gpu.cells_uploaded = 0;
        counters.cell_textures.fetch_add(1, Ordering::Relaxed);
    }
    if gpu.cells_uploaded == extracted.cells_generation {
        return;
    }
    if let Some((texture, _, _)) = &gpu.cells {
        write_cells(queue, texture, &extracted.cells, size, CELL_TEXEL_BYTES);
        gpu.cells_uploaded = extracted.cells_generation;
    }
}

/// A texture of one texel per map cell, `size` cells, in `format`, and its view.
fn cell_texture(
    device: &RenderDevice,
    label: &'static str,
    format: TextureFormat,
    size: UVec2,
) -> (Texture, TextureView, UVec2) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view, size)
}

/// Writes `bytes`, one texel of `texel_bytes` per cell row-major, into all of `texture`.
fn write_cells(
    queue: &RenderQueue,
    texture: &Texture,
    bytes: &[u8],
    size: UVec2,
    texel_bytes: u32,
) {
    queue.write_texture(
        ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        bytes,
        ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(size.x * texel_bytes),
            rows_per_image: Some(size.y),
        },
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
    );
}

/// Uploads the fog bytes when their generation moved, creating the texture for a new map size.
fn upload_fog(
    device: &RenderDevice,
    queue: &RenderQueue,
    gpu: &mut MinimapGpu,
    counters: &RenderCounters,
    extracted: &ExtractedMinimap,
) {
    let size = extracted.map_size;
    if size.x == 0 || size.y == 0 || extracted.fog.len() != size.x as usize * size.y as usize {
        return;
    }
    if gpu.fog.as_ref().is_none_or(|(_, _, held)| *held != size) {
        gpu.fog = Some(cell_texture(device, "minimap_fog", FOG_FORMAT, size));
        gpu.fog_uploaded = 0;
        counters.fog_textures.fetch_add(1, Ordering::Relaxed);
    }
    if gpu.fog_uploaded == extracted.fog_generation {
        return;
    }
    if let Some((texture, _, _)) = &gpu.fog {
        write_cells(queue, texture, &extracted.fog, size, 1);
        gpu.fog_uploaded = extracted.fog_generation;
    }
}

/// Uploads the marks when their generation moved, growing the buffer by doubling when outgrown.
fn upload_marks(
    device: &RenderDevice,
    queue: &RenderQueue,
    gpu: &mut MinimapGpu,
    counters: &RenderCounters,
    extracted: &ExtractedMinimap,
) {
    let needed = extracted.marks.len().max(1);
    if gpu.marks.is_none() || needed > gpu.mark_capacity {
        let mut capacity = gpu.mark_capacity.max(INITIAL_MARK_CAPACITY);
        while capacity < needed {
            capacity *= 2;
        }
        gpu.marks = Some(device.create_buffer(&BufferDescriptor {
            label: Some("minimap_marks"),
            size: (capacity * size_of::<MinimapMark>()) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        gpu.mark_capacity = capacity;
        gpu.marks_uploaded = 0;
        counters.mark_buffers.fetch_add(1, Ordering::Relaxed);
    }
    if gpu.marks_uploaded == extracted.marks_generation {
        return;
    }
    if let (Some(buffer), false) = (&gpu.marks, extracted.marks.is_empty()) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&extracted.marks));
    }
    gpu.mark_count = u32::try_from(extracted.marks.len()).unwrap_or(u32::MAX);
    gpu.marks_uploaded = extracted.marks_generation;
}

/// Label of [`MinimapNode`] in the top-level render graph.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct MinimapLabel;

/// Records the minimap pass: terrain, then marks, into the minimap image.
#[derive(Default)]
pub struct MinimapNode;

impl Node for MinimapNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext<'_>,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let (Some(gpu), Some(pipeline), Some(evidence)) = (
            world.get_resource::<MinimapGpu>(),
            world.get_resource::<MinimapPipeline>(),
            world.get_resource::<MinimapRenderEvidence>(),
        ) else {
            return Ok(());
        };
        let cache = world.resource::<PipelineCache>();
        let (Some(target), Some(group), Some(terrain), Some(marks)) = (
            &gpu.target,
            &gpu.bind_group,
            cache.get_render_pipeline(pipeline.terrain),
            cache.get_render_pipeline(pipeline.marks),
        ) else {
            return Ok(());
        };
        let diagnostics = render_context.diagnostic_recorder();
        let mut pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("minimap"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(LinearRgba::BLACK.into()),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let span = diagnostics.pass_span(&mut pass, "minimap");
        pass.set_bind_group(0, group, &[]);
        pass.set_render_pipeline(terrain);
        pass.draw(0..3, 0..1);
        if gpu.mark_count > 0 {
            pass.set_render_pipeline(marks);
            pass.draw(0..6, 0..gpu.mark_count);
        }
        span.end(&mut pass);
        evidence.0.frames.fetch_add(1, Ordering::Relaxed);
        evidence
            .0
            .drawn_marks
            .store(u64::from(gpu.mark_count), Ordering::Relaxed);
        Ok(())
    }
}
