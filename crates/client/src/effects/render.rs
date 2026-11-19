//! The instanced effect renderer: retained GPU buffers, three pipelines, per-run draws in the
//! transparent phase and one distortion draw in the transmissive phase (plan §5.1, §5.6 "Sort
//! and blend").
//!
//! # Responsibility
//! - **Buffers.** One storage buffer of [`SpriteInstance`] (sprites, rings and particles) and
//!   one of [`DecalInstance`], created once at the budget's capacity when the vocabulary is
//!   loaded; the cards are written in place during extraction (`RenderQueue::write_buffer`), as
//!   `vfx::gpu_upload` writes its slots. No asset is created or modified per frame.
//! - **Decal aging (plan `2026-11-19-retained-effect-instances` D-5).** The decals are not
//!   uploaded per frame. The render world keeps a copy of the main world's decal table: a row
//!   buffer of [`GpuDecalRecord`], the order of the rows, and the catalog's curve table. Each
//!   extraction copies the rows the table's last rewrite wrote (its patches), or every live row
//!   when it missed a generation, and the order when it moved. The compute node
//!   [`EffectDecalAgeNode`] runs in the `Core3d` graph between the prepass and the main opaque
//!   pass, for the view the effects draw in: `apply_patches` writes the patched rows,
//!   `age` (`decal_age.wgsl`, the port of `decal_table::age_decal`) writes one decal per row of
//!   the order into the decal buffer, which `decal.wgsl` draws unchanged.
//!   [`EffectDecalSource::Cpu`] (tests and `frame_budget --decal-source cpu` only) uploads the
//!   rows aged on the CPU into the same buffer instead and skips the aging dispatch.
//! - **Textures.** The drawable texel array of every listed texture set
//!   (`texture_2d_array`, mips included) and the ramp atlas (one 256-texel `Rgba32Float` row
//!   per ramp, built from data), created once per vocabulary.
//! - **Pipelines.** Cards: a camera-, ground- or line-oriented quad per instance, premultiplied
//!   blending (`ONE, ONE_MINUS_SRC_ALPHA`, so additive parts with alpha 0 share the pass),
//!   depth test against the opaque depth, no depth write, soft depth against the depth
//!   prepass. Decals: a box per instance drawn by its back faces with no depth test, the
//!   ground point reconstructed from the depth prepass and tested against the box.
//!   Distortion (WP7): a camera- or ground-oriented disc per instance that samples the view's
//!   transmission texture (Bevy's copy of the opaque scene, made before the transmissive
//!   phase) at an offset from scrolling noise and blends it over the scene inside a soft disc;
//!   queued as one `Transmissive3d` item, which also makes Bevy prepare and copy that texture
//!   for the frame (only when the camera's `screen_space_specular_transmission_steps` is
//!   above 0, Bevy's default 1).
//!
//! # Ordering against bubbles and other transparent items
//! Unit bubbles are `AlphaMode::Blend` and write no depth, so neither the depth test nor the
//! prepass can hide a card behind one; only the transparent phase's back-to-front order can
//! (the `WP4a` spike, `docs/evidence/effects/hanabi-spike.md` §6). One phase item per block
//! would sort the whole block as one item. Instead the cards, already sorted back to front
//! by the runtime, are cut into *runs* at the view distance of every other transparent item
//! of the view (bubbles, combat cards, the smog volume): each run becomes its own
//! `Transparent3d` item at the distance of its first card, drawing its instance range with one
//! draw call. The result is exactly the order one item per card would give, with at most one
//! more draw than there are other transparent items (capped at [`MAX_CARD_RUNS`]; past the
//! cap the last run takes the rest). Decals are ground marks: one item sorted before every
//! other transparent item, so bubbles and smog composite over them.
//! The bubble shader, its look and its goldens are untouched.
//!
//! # Invariants
//! - The view bind group needs the view's depth prepass; a view without one draws no effects.
//! - Effects are not tone-mapped: ramps author display-linear colours (HDR and bloom are the
//!   plan's D-11 decision, `WP6c`).
//! - Runs are queued for the first 3D view with a depth prepass only, and the decals are aged
//!   for that view only.
//! - The decal buffer holds one decal per row of the order: never more than the decal budget,
//!   because an effect has no more rows than its grant (`decal_table`).
//! - The rows on the GPU are the table's rows of the generation the node last applied; an
//!   extraction that finds a gap (the node did not run, a clear, a new vocabulary) copies every
//!   live row, so a missed frame never leaves a stale row drawn. A new `EffectRuntime` is made
//!   only with a new vocabulary version (`systems::load_effect_catalog`), whose rebuild forgets
//!   the applied generation, so the generations of two runtimes are never mixed.

use super::budget::RETIRE_SECS;
use super::catalog::EffectCatalog;
use super::curve::RAMP_TEXELS;
use super::decal_table::{
    curve_table, split_time, DecalRecord, DecalTable, GpuCurveKey, GpuDecalRecord, RowCopy,
};
use super::instances::{DecalInstance, SpriteInstance};
use super::ledger::MARK_RETIRE_SECS;
use super::runtime::{CameraView, EffectRuntime};
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::core_pipeline::core_3d::{
    Transmissive3d, Transparent3d, ViewTransmissionTexture, CORE_3D_DEPTH_FORMAT,
};
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::ecs::query::ROQueryItem;
use bevy::ecs::system::lifetimeless::{Read, SRes};
use bevy::ecs::system::SystemParamItem;
use bevy::image::BevyDefault;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_graph::{
    Node, NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel,
};
use bevy::render::render_phase::{
    sort_phase_system, AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
    RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
};
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only_sized, storage_buffer_sized, texture_2d, texture_2d_array,
    texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer, uniform_buffer_sized,
};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BlendComponent,
    BlendFactor, BlendOperation, BlendState, Buffer, BufferDescriptor, BufferUsages,
    CachedComputePipelineId, CachedPipelineState, ColorTargetState, ColorWrites, CompareFunction,
    ComputePassDescriptor, ComputePipelineDescriptor, DepthStencilState, Extent3d, Face,
    FragmentState, Maintain, MapMode, MultisampleState, PipelineCache, PipelineCacheError,
    PrimitiveState, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, SpecializedRenderPipeline, SpecializedRenderPipelines, TextureDataOrder,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension, VertexState,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSet};
use bytemuck::{Pod, Zeroable};
use std::num::NonZeroU64;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Most `Transparent3d` items the cards are cut into per frame.
pub const MAX_CARD_RUNS: usize = 64;
/// Vertices of one card (two triangles).
const CARD_VERTICES: u32 = 6;
/// Vertices of one decal box (twelve triangles).
const DECAL_VERTICES: u32 = 36;
/// Shader of the cards.
const CARD_SHADER: &str = "shaders/effects/sprite.wgsl";
/// Shader of the decals.
const DECAL_SHADER: &str = "shaders/effects/decal.wgsl";
/// Distortion shader, below the assets root.
const DISTORTION_SHADER: &str = "shaders/effects/distortion.wgsl";
/// Shared import of both.
const COMMON_SHADER: &str = "shaders/effects/common.wgsl";
/// Decal aging compute shader, below the assets root.
const DECAL_AGE_SHADER: &str = "shaders/effects/decal_age.wgsl";
/// Threads per workgroup of the aging entry points (`decal_age.wgsl`'s `WORKGROUP`).
const AGE_WORKGROUP: u32 = 64;

/// Bumped by every load of the vocabulary, so the render world rebuilds its resources.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectCatalogVersion(pub u64);

/// Registers extraction, the pipelines, queueing and the draw commands.
pub struct EffectRenderPlugin;

impl Plugin for EffectRenderPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<SpecializedRenderPipelines<EffectPipeline>>()
            .init_resource::<EffectRuns>()
            .add_render_command::<Transparent3d, DrawEffects>()
            .add_render_command::<Transmissive3d, DrawEffects>()
            .add_systems(ExtractSchedule, extract_effects)
            .add_render_graph_node::<EffectDecalAgeNode>(Core3d, EffectDecalAgeLabel)
            .add_render_graph_edges(
                Core3d,
                (Node3d::Prepass, EffectDecalAgeLabel, Node3d::MainOpaquePass),
            )
            .add_systems(
                Render,
                (
                    report_decal_age_pipelines.in_set(RenderSet::Queue),
                    queue_effects
                        .in_set(RenderSet::PhaseSort)
                        .before(sort_phase_system::<Transparent3d>),
                    queue_distortions
                        .in_set(RenderSet::PhaseSort)
                        .before(sort_phase_system::<Transmissive3d>),
                    prepare_effect_view_bind_groups.in_set(RenderSet::PrepareBindGroups),
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<EffectPipeline>()
            .init_resource::<DecalAgePipeline>();
        let world = render_app.world_mut();
        let cards = (0..MAX_CARD_RUNS)
            .map(|index| {
                world
                    .spawn(EffectRunSlot {
                        kind: RunKind::Cards,
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                    })
                    .id()
            })
            .collect();
        let decal = world
            .spawn(EffectRunSlot {
                kind: RunKind::Decals,
                index: 0,
            })
            .id();
        let darkening_cards = world
            .spawn(EffectRunSlot {
                kind: RunKind::Cards,
                index: DARKENING_RUN,
            })
            .id();
        let distortion = world
            .spawn(EffectRunSlot {
                kind: RunKind::Distortions,
                index: 0,
            })
            .id();
        world.insert_resource(EffectRunEntities {
            cards,
            decal,
            darkening_cards,
            distortion,
        });
        let gpu = EffectGpu::new(world.resource::<RenderDevice>());
        world.insert_resource(gpu);
    }
}

/// Which block a phase item draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RunKind {
    /// A run of cards.
    Cards,
    /// Every decal.
    Decals,
    /// Every distortion disc.
    Distortions,
}

/// Which run a phase item draws.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct EffectRunSlot {
    /// The block it draws.
    kind: RunKind,
    /// Index of the card run.
    index: u32,
}

/// The render-world entities phase items point at.
#[derive(Resource, Debug)]
struct EffectRunEntities {
    /// One per card run.
    cards: Vec<Entity>,
    /// The decal items.
    decal: Entity,
    /// The darkening cards' item.
    darkening_cards: Entity,
    /// The distortion item.
    distortion: Entity,
}

/// [`EffectRunSlot::index`] of the item that draws the darkening cards.
const DARKENING_RUN: u32 = u32::MAX;

/// This frame's instance ranges per run.
#[derive(Resource, Debug, Default)]
struct EffectRuns {
    /// Card ranges, indexed by [`EffectRunSlot::index`].
    cards: Vec<Range<u32>>,
    /// The darkening cards: the first cards of the sorted buffer.
    darkening: Range<u32>,
    /// The decal range.
    decals: Range<u32>,
    /// Distances of the view's other transparent items, sorted (retained scratch).
    others: Vec<f32>,
}

/// Where the decals the renderer draws are aged (plan `2026-11-19-retained-effect-instances`
/// D-10). A main-world resource the effect plugin inserts as [`Self::Gpu`]; only tests and
/// `frame_budget --decal-source cpu` choose [`Self::Cpu`]. It is a diagnostic mode, never a
/// fallback: a compute pipeline that fails is reported, not replaced.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EffectDecalSource {
    /// The compute pass ages the retained rows (production).
    #[default]
    Gpu,
    /// The main world ages the rows with the Rust reference ([`EffectRuntime::age_decals_on_cpu`])
    /// and uploads them into the same decal buffer; the aging dispatch is skipped.
    Cpu,
}

/// The aging pass's inputs of a frame: `decal_age.wgsl`'s `DecalAgeParams` byte for byte.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Pod, Zeroable)]
pub struct DecalAgeUniform {
    /// The frame's time, seconds of `Time<Virtual>`, split by [`split_time`].
    pub now: [f32; 2],
    /// The level-of-detail threshold, pixels.
    pub min_screen_px: f32,
    /// 1 when the frame has a camera (level of detail applies), 0 when it has none.
    pub has_camera: u32,
    /// The camera's position, world space.
    pub camera_position: [f32; 3],
    /// `tan(fov_y / 2)` of the camera.
    pub half_tan: f32,
    /// The camera's unit view direction.
    pub camera_forward: [f32; 3],
    /// The viewport's height, pixels.
    pub viewport_px: f32,
    /// Patches to apply this frame.
    pub patches: u32,
    /// Rows of the order to age this frame.
    pub rows: u32,
    /// Curves in the curve table.
    pub curves: u32,
    /// Always 0: the shader's optimization barrier (`decal_age.wgsl`'s `opaque`) XORs it into
    /// values the driver must not simplify across.
    pub zero_bits: u32,
    /// Seconds retired detail erodes out over ([`RETIRE_SECS`]).
    pub retire_secs: f32,
    /// Seconds retired marks erode out over ([`MARK_RETIRE_SECS`]).
    pub mark_retire_secs: f32,
    /// Padding to a whole 16-byte row.
    pub pad: [u32; 2],
}

impl DecalAgeUniform {
    /// The inputs for aging `rows` rows and applying `patches` patches at `now` from `camera`.
    #[must_use]
    pub fn new(
        now: f64,
        camera: Option<&CameraView>,
        catalog: &EffectCatalog,
        (patches, rows): (u32, u32),
    ) -> Self {
        Self {
            now: split_time(now),
            min_screen_px: catalog.budget.min_screen_px,
            has_camera: u32::from(camera.is_some()),
            camera_position: camera.map_or([0.0; 3], |camera| camera.position.to_array()),
            half_tan: camera.map_or(0.0, CameraView::half_tan),
            camera_forward: camera.map_or([0.0; 3], |camera| camera.forward.to_array()),
            viewport_px: camera.map_or(0.0, |camera| camera.viewport_px),
            patches,
            rows,
            curves: u32::try_from(catalog.curves.len()).unwrap_or(u32::MAX),
            zero_bits: 0,
            retire_secs: RETIRE_SECS,
            mark_retire_secs: MARK_RETIRE_SECS,
            pad: [0; 2],
        }
    }
}

/// One row the aging pass copies into the row buffer: `decal_age.wgsl`'s `DecalPatch`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Pod, Zeroable)]
pub struct GpuDecalPatch {
    /// The row it writes.
    pub row: u32,
    /// Padding to the record's 16-byte alignment.
    pub pad: [u32; 3],
    /// The row's record.
    pub record: GpuDecalRecord,
}

/// One frame's decal rows aged by the GPU, read back beside the Rust reference of the same
/// rows, time and camera (D-10 check 1).
#[derive(Debug, Clone, PartialEq)]
pub struct DecalProbeFrame {
    /// The frame's time, seconds of `Time<Virtual>`.
    pub now: f64,
    /// The frame's camera.
    pub camera: Option<CameraView>,
    /// The rows of the order, in order.
    pub records: Vec<DecalRecord>,
    /// `decal_table::age_decal` of each row (a row that draws nothing as
    /// `decal_table::hidden_decal`).
    pub expected: Vec<DecalInstance>,
    /// What the compute pass wrote for each row.
    pub aged: Vec<DecalInstance>,
    /// Whether this frame copied every live row rather than the patches (a gap in the
    /// generations).
    pub full_copy: bool,
}

/// Test instrumentation of the decal aging (`decal_age_gpu_tests`, D-10). A main-world
/// resource that production never inserts, read only by [`EffectDecalProbePlugin`]'s systems,
/// which wait for the GPU and allocate per frame.
#[derive(Resource, Debug, Clone, Default)]
pub struct EffectDecalProbe {
    /// With `read_back`, every frame whose decals were aged on the GPU appends a
    /// [`DecalProbeFrame`] here.
    pub frames: Arc<Mutex<Vec<DecalProbeFrame>>>,
    /// Whether frames are read back.
    pub read_back: bool,
    /// When above 0, every this-many-th extraction forgets which generation the GPU's rows
    /// hold, as a render world that lost track of it, so that frame copies every live row:
    /// the full resync D-5 promises draws the same pictures as the patches.
    pub forget_every: u32,
}

/// The render world's side of [`EffectDecalProbe`].
#[derive(Resource, Default)]
struct DecalProbeState {
    /// Where finished frames go.
    sink: Option<Arc<Mutex<Vec<DecalProbeFrame>>>>,
    /// This frame's reference, waiting for the GPU's output.
    pending: Option<DecalProbeFrame>,
    /// The mappable copy of the decal buffer.
    staging: Option<Buffer>,
    /// Extractions so far under a probe.
    extractions: u64,
    /// Whether this extraction forgets the generation the GPU's rows hold.
    forget: bool,
    /// Whether the node aged this frame's rows and copied them out (set by the node, which
    /// reads the world immutably): a frame the node skipped, its pipelines still compiling,
    /// is not read back.
    copied: std::sync::atomic::AtomicBool,
}

/// The state of the aging pass's compute pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgePipelines {
    /// Not compiled yet: the decals are not drawn.
    Compiling,
    /// Both compiled.
    Ready,
    /// One failed; reported once, and the decals are never drawn.
    Failed,
}

/// Test instrumentation of the decal aging: registers [`EffectDecalProbe`]'s render-world
/// systems (read-back after the frame's submission, fault injection). Only tests add it, after
/// the effect plugins; the production renderer has none of it (independent review round 2,
/// S3).
pub struct EffectDecalProbePlugin;

impl Plugin for EffectDecalProbePlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<DecalProbeState>()
            .add_systems(ExtractSchedule, extract_decal_probe.before(extract_effects))
            .add_systems(Render, read_decal_probe.in_set(RenderSet::Cleanup));
    }
}

/// GPU resources of the loaded vocabulary and this frame's counts.
#[derive(Resource)]
struct EffectGpu {
    /// Version of the vocabulary they were built from.
    version: Option<u64>,
    /// Card storage buffer.
    cards: Option<Buffer>,
    /// Decal storage buffer.
    decals: Option<Buffer>,
    /// Distortion storage buffer.
    distortions: Option<Buffer>,
    /// Bind group of the card pipeline's resources.
    card_group: Option<BindGroup>,
    /// Bind group of the decal pipeline's resources.
    decal_group: Option<BindGroup>,
    /// Bind group of the distortion pipeline's resources.
    distortion_group: Option<BindGroup>,
    /// Shared sampler.
    sampler: Sampler,
    /// Cards to draw this frame.
    card_count: u32,
    /// Decals to draw this frame.
    decal_count: u32,
    /// Distortion discs to draw this frame.
    distortion_count: u32,
    /// Cards this frame that darken what lies under them ([`super::instances::FLAG_DARKEN`]):
    /// the first this many of the card buffer.
    darkening_cards: u32,
    /// Whether any decal this frame darkens what lies under it.
    darkening_decals: bool,
    /// Card centres in draw order (retained), for the runs.
    centers: Vec<Vec3>,
    /// The retained decal rows ([`GpuDecalRecord`]), at the decal budget's capacity.
    rows: Option<Buffer>,
    /// The order of the rows (`u32` row indices).
    order: Option<Buffer>,
    /// This frame's patches ([`GpuDecalPatch`]).
    patches: Option<Buffer>,
    /// The aging pass's [`DecalAgeUniform`].
    age_uniform: Option<Buffer>,
    /// Bind group of the aging pass.
    age_group: Option<BindGroup>,
    /// This frame's patches as the GPU reads them (retained, at the budget's capacity).
    patch_staging: Vec<GpuDecalPatch>,
    /// This frame's aging inputs.
    uniform: DecalAgeUniform,
    /// Whether the aging dispatch runs this frame (the rows changed or time moved, under
    /// [`EffectDecalSource::Gpu`]).
    age_rows: bool,
    /// The table generation whose patches this frame applies.
    pending_generation: u64,
    /// One more than the table generation the node last applied (0: none): written by the
    /// node, which reads the world immutably, and read at the next extraction.
    applied_generation: AtomicU64,
    /// The order generation the order buffer holds.
    order_generation: Option<u64>,
    /// Whether the aging pipelines are compiled (the decals are drawn only then) or failed.
    age_pipelines: AgePipelines,
    /// Under [`EffectDecalSource::Gpu`]; the decal buffer then holds the compute pass's output.
    age_on_gpu: bool,
    /// The render-world view the effects are drawn in this frame (the node ages for it only).
    age_view: Option<Entity>,
}

impl EffectGpu {
    /// Empty resources with the shared sampler.
    fn new(device: &RenderDevice) -> Self {
        Self {
            version: None,
            cards: None,
            decals: None,
            distortions: None,
            card_group: None,
            decal_group: None,
            distortion_group: None,
            sampler: device.create_sampler(&SamplerDescriptor {
                label: Some("effect_sampler"),
                mag_filter: bevy::render::render_resource::FilterMode::Linear,
                min_filter: bevy::render::render_resource::FilterMode::Linear,
                mipmap_filter: bevy::render::render_resource::FilterMode::Linear,
                // Breakup noise tiles; drawable cells have empty borders, so repeating
                // addressing never shows a neighbour's texels on them.
                address_mode_u: bevy::render::render_resource::AddressMode::Repeat,
                address_mode_v: bevy::render::render_resource::AddressMode::Repeat,
                ..default()
            }),
            card_count: 0,
            decal_count: 0,
            distortion_count: 0,
            darkening_cards: 0,
            darkening_decals: false,
            centers: Vec::new(),
            rows: None,
            order: None,
            patches: None,
            age_uniform: None,
            age_group: None,
            patch_staging: Vec::new(),
            uniform: DecalAgeUniform::default(),
            age_rows: false,
            pending_generation: 0,
            applied_generation: AtomicU64::new(0),
            order_generation: None,
            age_pipelines: AgePipelines::Compiling,
            age_on_gpu: true,
            age_view: None,
        }
    }

    /// Builds buffers, textures and bind groups for `catalog`.
    fn rebuild(
        &mut self,
        catalog: &EffectCatalog,
        device: &RenderDevice,
        queue: &RenderQueue,
        (layout, age_layout): (&BindGroupLayout, &BindGroupLayout),
    ) {
        self.card_group = None;
        self.decal_group = None;
        self.distortion_group = None;
        let budget = &catalog.budget;
        let card_capacity = u64::from(budget.sprites + budget.particles).max(1);
        let decal_capacity = u64::from(budget.decals).max(1);
        let buffer = |label: &'static str, size: u64| {
            device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let cards = buffer(
            "effect_cards",
            card_capacity * size_of::<SpriteInstance>() as u64,
        );
        // Written by the aging pass (storage), by the CPU source (copy), and copied out by the
        // decal probe.
        let decals = device.create_buffer(&BufferDescriptor {
            label: Some("effect_decals"),
            size: decal_capacity * size_of::<DecalInstance>() as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.rebuild_aging(catalog, device, queue, age_layout, &decals);
        let distortions = buffer(
            "effect_distortions",
            u64::from(budget.distortions).max(1) * size_of::<SpriteInstance>() as u64,
        );
        self.centers = Vec::with_capacity(usize::try_from(card_capacity).unwrap_or(0));
        if let (Some(atlas), Some(ramps)) = (
            atlas_view(catalog, device, queue),
            ramp_view(catalog, device, queue),
        ) {
            let group = |buffer: &Buffer| {
                device.create_bind_group(
                    "effect_resources",
                    layout,
                    &BindGroupEntries::sequential((
                        buffer.as_entire_binding(),
                        &atlas,
                        &self.sampler,
                        &ramps,
                    )),
                )
            };
            self.card_group = Some(group(&cards));
            self.decal_group = Some(group(&decals));
            self.distortion_group = Some(group(&distortions));
        }
        self.cards = Some(cards);
        self.decals = Some(decals);
        self.distortions = Some(distortions);
    }

    /// Builds the aging pass's buffers and bind group for `catalog`, writing `decals`: the row,
    /// order and patch buffers at the decal budget's capacity, the curve table, the uniform.
    /// The rows are copied in full at the next extraction.
    fn rebuild_aging(
        &mut self,
        catalog: &EffectCatalog,
        device: &RenderDevice,
        queue: &RenderQueue,
        layout: &BindGroupLayout,
        decals: &Buffer,
    ) {
        let capacity = catalog.budget.decals.max(1);
        let storage = |label: &'static str, size: usize| {
            device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: u64::try_from(size).unwrap_or(u64::MAX).max(16),
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let rows = storage(
            "effect_decal_rows",
            capacity as usize * size_of::<GpuDecalRecord>(),
        );
        let order = storage("effect_decal_order", capacity as usize * size_of::<u32>());
        let patches = storage(
            "effect_decal_patches",
            capacity as usize * size_of::<GpuDecalPatch>(),
        );
        let (ranges, keys) = curve_table(catalog);
        let ranges_buffer = storage("effect_curve_ranges", ranges.len() * size_of::<[u32; 2]>());
        let keys_buffer = storage("effect_curve_keys", keys.len() * size_of::<GpuCurveKey>());
        if !ranges.is_empty() {
            queue.write_buffer(&ranges_buffer, 0, bytemuck::cast_slice(&ranges));
        }
        if !keys.is_empty() {
            queue.write_buffer(&keys_buffer, 0, bytemuck::cast_slice(&keys));
        }
        let uniform = device.create_buffer(&BufferDescriptor {
            label: Some("effect_decal_age"),
            size: size_of::<DecalAgeUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.age_group = Some(device.create_bind_group(
            "effect_decal_age",
            layout,
            &BindGroupEntries::sequential((
                uniform.as_entire_binding(),
                rows.as_entire_binding(),
                order.as_entire_binding(),
                patches.as_entire_binding(),
                ranges_buffer.as_entire_binding(),
                keys_buffer.as_entire_binding(),
                decals.as_entire_binding(),
            )),
        ));
        self.rows = Some(rows);
        self.order = Some(order);
        self.patches = Some(patches);
        self.age_uniform = Some(uniform);
        self.patch_staging = Vec::with_capacity(capacity as usize);
        self.applied_generation.store(0, Ordering::Relaxed);
        self.order_generation = None;
    }
}

/// The drawable texel array as a view, if the vocabulary lists any drawable set.
fn atlas_view(
    catalog: &EffectCatalog,
    device: &RenderDevice,
    queue: &RenderQueue,
) -> Option<TextureView> {
    let atlas = &catalog.atlas;
    if atlas.layers == 0 || atlas.texels.is_empty() {
        return None;
    }
    let texture = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("effect_atlas"),
            size: Extent3d {
                width: atlas.cell_size,
                height: atlas.cell_size,
                depth_or_array_layers: atlas.layers,
            },
            mip_level_count: atlas.mip_count,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        TextureDataOrder::LayerMajor,
        &atlas.texels,
    );
    Some(texture.create_view(&TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    }))
}

/// The ramp atlas as a view, if the vocabulary has any ramp.
fn ramp_view(
    catalog: &EffectCatalog,
    device: &RenderDevice,
    queue: &RenderQueue,
) -> Option<TextureView> {
    if catalog.ramps.is_empty() {
        return None;
    }
    let texels: Vec<f32> = catalog
        .ramps
        .iter()
        .flat_map(|ramp| ramp.texels().iter().flatten().copied())
        .collect();
    let texture = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("effect_ramps"),
            size: Extent3d {
                width: u32::try_from(RAMP_TEXELS).unwrap_or(256),
                height: u32::try_from(catalog.ramps.len()).unwrap_or(u32::MAX),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba32Float,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        TextureDataOrder::LayerMajor,
        bytemuck::cast_slice(&texels),
    );
    Some(texture.create_view(&TextureViewDescriptor::default()))
}

/// Copies this frame's cards and distortions into the retained buffers, and the decal
/// table's changes into the aging pass's (D-5).
// PERFORMANCE RATIONALE:
// Why: Effects change every frame; recreating buffers or touching assets per frame would
// allocate GPU memory and bind groups continuously (the vfx C-17 finding).
// What: The buffers are made once per vocabulary at the budget's capacity; each frame writes
// only the live prefix with one `write_buffer` per block and copies the card centres into a
// retained vector. The decals are not uploaded: only the rows their table rewrote.
// Trade-off: The capacity is reserved on the GPU even when few effects are live.
#[expect(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes system parameters by value, each a separate declared dependency"
)]
fn extract_effects(
    catalog: Extract<'_, '_, Option<Res<'_, EffectCatalog>>>,
    version: Extract<'_, '_, Option<Res<'_, EffectCatalogVersion>>>,
    runtime: Extract<'_, '_, Option<Res<'_, EffectRuntime>>>,
    source: Extract<'_, '_, Option<Res<'_, EffectDecalSource>>>,
    (mut gpu, mut probe): (ResMut<'_, EffectGpu>, Option<ResMut<'_, DecalProbeState>>),
    pipeline: Res<'_, EffectPipeline>,
    age_pipeline: Res<'_, DecalAgePipeline>,
    (device, queue): (Res<'_, RenderDevice>, Res<'_, RenderQueue>),
) {
    gpu.uniform.patches = 0;
    gpu.age_rows = false;
    let (Some(catalog), Some(version), Some(runtime)) = (&*catalog, &*version, &*runtime) else {
        gpu.card_count = 0;
        gpu.decal_count = 0;
        gpu.distortion_count = 0;
        return;
    };
    if gpu.version != Some(version.0) {
        gpu.rebuild(
            catalog,
            &device,
            &queue,
            (&pipeline.resource_layout, &age_pipeline.layout),
        );
        gpu.version = Some(version.0);
    }
    let frame = &runtime.frame;
    let gpu = &mut *gpu;
    let table = runtime.decal_table();
    gpu.age_on_gpu = source.as_deref().copied().unwrap_or_default() == EffectDecalSource::Gpu;
    gpu.card_count = u32::try_from(frame.sorted.len()).unwrap_or(0);
    gpu.decal_count = if frame.draw_decals {
        u32::try_from(table.order().len()).unwrap_or(0)
    } else {
        0
    };
    gpu.distortion_count = u32::try_from(frame.distortions.len()).unwrap_or(0);
    gpu.darkening_cards = frame.darkening_cards.min(gpu.card_count);
    gpu.darkening_decals = table.darkening() > 0;
    if !runtime.is_changed() {
        return;
    }
    if let (Some(buffer), false) = (&gpu.cards, frame.sorted.is_empty()) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&frame.sorted));
    }
    if let (Some(buffer), false) = (&gpu.distortions, frame.distortions.is_empty()) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&frame.distortions));
    }
    if probe.as_ref().is_some_and(|probe| probe.forget) {
        gpu.applied_generation.store(0, Ordering::Relaxed);
    }
    let full_copy = extract_decal_rows(gpu, runtime, catalog, &queue);
    if let Some(pending) = probe.as_mut().and_then(|probe| probe.pending.as_mut()) {
        pending.full_copy = full_copy;
    }
    gpu.centers.clear();
    gpu.centers.extend(
        frame
            .sorted
            .iter()
            .map(|card| Vec3::from_array(card.center)),
    );
}

/// Copies the decal table's rows written since the generation the node last applied (its
/// patches, or every live row after a gap), its order when it moved, and the frame's aging
/// inputs; under [`EffectDecalSource::Cpu`] uploads the rows aged on the CPU instead of
/// asking for the aging dispatch. Returns whether it copied every live row.
fn extract_decal_rows(
    gpu: &mut EffectGpu,
    runtime: &EffectRuntime,
    catalog: &EffectCatalog,
    queue: &RenderQueue,
) -> bool {
    let table: &DecalTable = runtime.decal_table();
    let generation = table.generation();
    let applied = gpu
        .applied_generation
        .load(Ordering::Relaxed)
        .checked_sub(1);
    let copy = table.copy_since(applied);
    let missing = stage_patches(table, copy, &mut gpu.patch_staging);
    if missing > 0 {
        error!("effect decals: {missing} listed decal rows have no record (a table defect)");
    }
    let resync = copy == RowCopy::Every;
    if let (Some(buffer), false) = (&gpu.patches, gpu.patch_staging.is_empty()) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&gpu.patch_staging));
    }
    if resync || gpu.order_generation != Some(table.order_generation()) {
        if let (Some(buffer), false) = (&gpu.order, table.order().is_empty()) {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(table.order()));
        }
        gpu.order_generation = Some(table.order_generation());
    }
    gpu.pending_generation = generation;
    let rows = u32::try_from(table.order().len()).unwrap_or(0);
    let patches = u32::try_from(gpu.patch_staging.len()).unwrap_or(0);
    gpu.uniform = DecalAgeUniform::new(
        runtime.last_build().unwrap_or(0.0),
        runtime.last_camera(),
        catalog,
        (patches, rows),
    );
    if let Some(buffer) = &gpu.age_uniform {
        queue.write_buffer(buffer, 0, bytemuck::bytes_of(&gpu.uniform));
    }
    gpu.age_rows = gpu.age_on_gpu && rows > 0;
    let aged = &runtime.frame.aged_decals;
    if let (false, Some(buffer), false) = (gpu.age_on_gpu, &gpu.decals, aged.is_empty()) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(aged));
    }
    resync
}

/// Writes into `staging` the rows `copy` asks for, in the GPU's form: the table's patches, or
/// every live row. `staging` is retained at the decal budget's capacity, which neither can
/// exceed, so this allocates nothing. Returns how many listed rows had no record (none can:
/// the table lists only its own rows; a defect is reported, not dropped silently).
fn stage_patches(table: &DecalTable, copy: RowCopy, staging: &mut Vec<GpuDecalPatch>) -> usize {
    let rows = match copy {
        RowCopy::Every => table.order(),
        RowCopy::Patches => table.patches(),
        RowCopy::Nothing => &[],
    };
    staging.clear();
    let mut missing = 0;
    for &row in rows {
        match table.record(row) {
            Some(record) => staging.push(GpuDecalPatch {
                row,
                pad: [0; 3],
                record: GpuDecalRecord::of(record),
            }),
            None => missing += 1,
        }
    }
    missing
}

/// Under [`EffectDecalProbe`]: decides whether this extraction forgets the generation the
/// GPU's rows hold, and with `read_back` keeps this frame's rows and their Rust reference for
/// the GPU's output, which [`read_decal_probe`] reads back after the frame is submitted.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn extract_decal_probe(
    probe: Extract<'_, '_, Option<Res<'_, EffectDecalProbe>>>,
    catalog: Extract<'_, '_, Option<Res<'_, EffectCatalog>>>,
    runtime: Extract<'_, '_, Option<Res<'_, EffectRuntime>>>,
    source: Extract<'_, '_, Option<Res<'_, EffectDecalSource>>>,
    device: Res<'_, RenderDevice>,
    mut state: ResMut<'_, DecalProbeState>,
) {
    state.pending = None;
    state.forget = false;
    let (Some(probe), Some(catalog), Some(runtime)) = (&*probe, &*catalog, &*runtime) else {
        return;
    };
    state.extractions += 1;
    state.forget = probe.forget_every > 0
        && state
            .extractions
            .is_multiple_of(u64::from(probe.forget_every));
    state.sink = Some(probe.frames.clone());
    let on_gpu = source.as_deref().copied().unwrap_or_default() == EffectDecalSource::Gpu;
    let table = runtime.decal_table();
    let Some(now) = runtime.last_build() else {
        return;
    };
    if !probe.read_back || !on_gpu || !runtime.is_changed() || table.order().is_empty() {
        return;
    }
    let bytes = u64::from(catalog.budget.decals.max(1)) * size_of::<DecalInstance>() as u64;
    if state
        .staging
        .as_ref()
        .is_none_or(|staging| staging.size() != bytes)
    {
        state.staging = Some(device.create_buffer(&BufferDescriptor {
            label: Some("effect_decal_probe"),
            size: bytes,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    let mut expected = Vec::new();
    table.age_rows(now, catalog, runtime.last_camera(), &mut expected);
    let records = table
        .order()
        .iter()
        .filter_map(|&row| table.record(row).copied())
        .collect();
    state.pending = Some(DecalProbeFrame {
        now,
        camera: runtime.last_camera().copied(),
        records,
        expected,
        aged: Vec::new(),
        full_copy: false,
    });
}

/// Reads the probe's copy of the decal buffer once the frame's commands are done, and hands
/// the frame to the test.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn read_decal_probe(device: Res<'_, RenderDevice>, mut state: ResMut<'_, DecalProbeState>) {
    let Some(mut frame) = state.pending.take() else {
        return;
    };
    if !state.copied.swap(false, Ordering::Relaxed) {
        return;
    }
    let (Some(staging), Some(sink)) = (&state.staging, &state.sink) else {
        return;
    };
    let bytes = (frame.expected.len() * size_of::<DecalInstance>()) as u64;
    if bytes == 0 {
        return;
    }
    let slice = staging.slice(0..bytes);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(MapMode::Read, move |result| {
        // The receiver waits below; a send fails only when it is gone, and then nobody reads.
        sender.send(result).ok();
    });
    device.poll(Maintain::Wait);
    match receiver.recv() {
        Ok(Ok(())) => {
            frame.aged = bytemuck::cast_slice(&slice.get_mapped_range()).to_vec();
            staging.unmap();
        }
        Ok(Err(error)) => {
            error!("effect decal probe: the readback failed to map: {error}");
            return;
        }
        Err(error) => {
            error!("effect decal probe: the readback never completed: {error}");
            return;
        }
    }
    match sink.lock() {
        Ok(mut frames) => frames.push(frame),
        Err(error) => error!("effect decal probe: the test's frame list is poisoned: {error}"),
    }
}

/// The aging pass's layout and its two compute pipelines.
#[derive(Resource)]
struct DecalAgePipeline {
    /// Uniform, rows, order, patches, curve ranges, curve keys, decals.
    layout: BindGroupLayout,
    /// `apply_patches`.
    apply: CachedComputePipelineId,
    /// `age`.
    age: CachedComputePipelineId,
}

impl FromWorld for DecalAgePipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let layout = device.create_bind_group_layout(
            "effect_decal_age",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer_sized(
                        false,
                        NonZeroU64::new(size_of::<DecalAgeUniform>() as u64),
                    ),
                    storage_buffer_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_sized(false, None),
                ),
            ),
        );
        let shader = world.resource::<AssetServer>().load(DECAL_AGE_SHADER);
        let cache = world.resource::<PipelineCache>();
        let compute = |entry: &'static str| {
            cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some(format!("effect_decal_{entry}").into()),
                layout: vec![layout.clone()],
                push_constant_ranges: Vec::new(),
                shader: shader.clone(),
                shader_defs: Vec::new(),
                entry_point: entry.into(),
                zero_initialize_workgroup_memory: false,
            })
        };
        let apply = compute("apply_patches");
        let age = compute("age");
        Self { layout, apply, age }
    }
}

/// Notes when the aging pipelines are compiled, so the decals are drawn only once the pass can
/// age them, and reports a pipeline that failed (an error, not a reason to fall back: the
/// decals are then never drawn).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn report_decal_age_pipelines(
    cache: Res<'_, PipelineCache>,
    pipeline: Res<'_, DecalAgePipeline>,
    mut gpu: ResMut<'_, EffectGpu>,
) {
    if gpu.age_pipelines != AgePipelines::Compiling {
        return;
    }
    // A queued pipeline enters the cache's list only when the cache next processes its queue;
    // `get_compute_pipeline_state` indexes that list, so it is asked only once both are in it.
    let listed = cache.pipelines().count();
    if pipeline.apply.id().max(pipeline.age.id()) >= listed {
        return;
    }
    let states = [
        cache.get_compute_pipeline_state(pipeline.apply),
        cache.get_compute_pipeline_state(pipeline.age),
    ];
    for state in states {
        match state {
            // A shader still loading is retried by the cache's next pass (independent review
            // round 2, B1): not a failure.
            CachedPipelineState::Err(
                PipelineCacheError::ShaderNotLoaded(_)
                | PipelineCacheError::ShaderImportNotYetAvailable,
            ) => return,
            CachedPipelineState::Err(error) => {
                error!("effect decals: the aging pipeline ({DECAL_AGE_SHADER}) failed: {error}");
                gpu.age_pipelines = AgePipelines::Failed;
                return;
            }
            _ => {}
        }
    }
    if states
        .iter()
        .all(|state| matches!(state, CachedPipelineState::Ok(_)))
    {
        gpu.age_pipelines = AgePipelines::Ready;
    }
}

/// Label of [`EffectDecalAgeNode`] in the `Core3d` graph.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct EffectDecalAgeLabel;

/// The decal aging pass (D-5): between the prepass and the main opaque pass of the view the
/// effects are drawn in, applies the frame's patches to the row buffer and ages the rows of
/// the order into the decal buffer.
#[derive(Default)]
pub struct EffectDecalAgeNode;

impl Node for EffectDecalAgeNode {
    fn run<'w>(
        &self,
        graph: &mut RenderGraphContext<'_>,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let gpu = world.resource::<EffectGpu>();
        if gpu.age_view.is_none() || graph.get_view_entity() != gpu.age_view {
            return Ok(());
        }
        let pipeline = world.resource::<DecalAgePipeline>();
        let cache = world.resource::<PipelineCache>();
        let (Some(group), Some(apply), Some(age)) = (
            &gpu.age_group,
            cache.get_compute_pipeline(pipeline.apply),
            cache.get_compute_pipeline(pipeline.age),
        ) else {
            return Ok(());
        };
        let patches = gpu.uniform.patches;
        let rows = if gpu.age_rows { gpu.uniform.rows } else { 0 };
        let diagnostics = render_context.diagnostic_recorder();
        let encoder = render_context.command_encoder();
        if patches > 0 || rows > 0 {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("effect_decal_age"),
                timestamp_writes: None,
            });
            let span = diagnostics.pass_span(&mut pass, "effect_decal_age");
            pass.set_bind_group(0, group, &[]);
            if patches > 0 {
                pass.set_pipeline(apply);
                pass.dispatch_workgroups(patches.div_ceil(AGE_WORKGROUP), 1, 1);
            }
            if rows > 0 {
                pass.set_pipeline(age);
                pass.dispatch_workgroups(rows.div_ceil(AGE_WORKGROUP), 1, 1);
            }
            span.end(&mut pass);
        }
        gpu.applied_generation
            .store(gpu.pending_generation + 1, Ordering::Relaxed);
        let Some(probe) = world.get_resource::<DecalProbeState>() else {
            return Ok(());
        };
        if let (Some(_), Some(staging), Some(decals), true) =
            (&probe.pending, &probe.staging, &gpu.decals, rows > 0)
        {
            let bytes = u64::from(rows) * size_of::<DecalInstance>() as u64;
            encoder.copy_buffer_to_buffer(decals, 0, staging, 0, bytes);
            probe.copied.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}

/// The two effect pipelines' shared layouts and shaders.
#[derive(Resource)]
struct EffectPipeline {
    /// View group: uniforms and the prepass depth, single-sampled.
    view_layout: BindGroupLayout,
    /// View group with a multisampled prepass depth.
    view_layout_msaa: BindGroupLayout,
    /// View group of the distortion pipeline: uniforms, the transmission texture, a sampler.
    distortion_view_layout: BindGroupLayout,
    /// Resource group: instances, atlas, sampler, ramps.
    resource_layout: BindGroupLayout,
    /// Card shader.
    card_shader: Handle<Shader>,
    /// Decal shader.
    decal_shader: Handle<Shader>,
    /// Distortion shader.
    distortion_shader: Handle<Shader>,
    /// The shared import, held so the composer keeps it.
    _common_shader: Handle<Shader>,
}

impl FromWorld for EffectPipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let view_entries = |msaa: bool| {
            BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer::<ViewUniform>(true),
                    if msaa {
                        texture_depth_2d_multisampled()
                    } else {
                        texture_depth_2d()
                    },
                ),
            )
        };
        let view_layout = device.create_bind_group_layout("effect_view", &view_entries(false));
        let view_layout_msaa =
            device.create_bind_group_layout("effect_view_msaa", &view_entries(true));
        let distortion_view_layout = device.create_bind_group_layout(
            "effect_distortion_view",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer::<ViewUniform>(true),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                ),
            ),
        );
        let resource_layout = device.create_bind_group_layout(
            "effect_resources",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    // Unsized: the card and decal pipelines bind arrays of different records.
                    storage_buffer_read_only_sized(false, None),
                    texture_2d_array(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                ),
            ),
        );
        let assets = world.resource::<AssetServer>();
        Self {
            view_layout,
            view_layout_msaa,
            distortion_view_layout,
            resource_layout,
            card_shader: assets.load(CARD_SHADER),
            decal_shader: assets.load(DECAL_SHADER),
            distortion_shader: assets.load(DISTORTION_SHADER),
            _common_shader: assets.load(COMMON_SHADER),
        }
    }
}

/// What a specialised effect pipeline depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EffectPipelineKey {
    /// The block the pipeline draws.
    kind: RunKind,
    /// MSAA sample count of the view.
    samples: u32,
    /// Whether the view renders to an HDR target.
    hdr: bool,
    /// The darkening pass: draws only the elements that darken what lies under them and
    /// multiplies the target by their output (`DARKEN_PASS`); the other pass draws the rest.
    darken: bool,
}

impl SpecializedRenderPipeline for EffectPipeline {
    type Key = EffectPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let decal = key.kind == RunKind::Decals;
        let shader = match key.kind {
            RunKind::Cards => self.card_shader.clone(),
            RunKind::Decals => self.decal_shader.clone(),
            RunKind::Distortions => self.distortion_shader.clone(),
        };
        let mut defs = Vec::new();
        if key.samples > 1 {
            defs.push("MULTISAMPLED".into());
        }
        if key.darken {
            defs.push("DARKEN_PASS".into());
        }
        let view_layout = match (key.kind, key.samples > 1) {
            (RunKind::Distortions, _) => self.distortion_view_layout.clone(),
            (_, true) => self.view_layout_msaa.clone(),
            (_, false) => self.view_layout.clone(),
        };
        let premultiplied = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        };
        // The darkening pass multiplies the target's colour by the shader's output and leaves
        // its alpha alone.
        let blend = if key.darken {
            BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::Dst,
                    dst_factor: BlendFactor::Zero,
                    operation: BlendOperation::Add,
                },
                alpha: BlendComponent {
                    src_factor: BlendFactor::Zero,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
            }
        } else {
            BlendState {
                color: premultiplied,
                alpha: premultiplied,
            }
        };
        RenderPipelineDescriptor {
            label: Some(
                match key.kind {
                    RunKind::Cards => "effect_cards",
                    RunKind::Decals => "effect_decals",
                    RunKind::Distortions => "effect_distortions",
                }
                .into(),
            ),
            layout: vec![view_layout, self.resource_layout.clone()],
            push_constant_ranges: Vec::new(),
            vertex: VertexState {
                shader: shader.clone(),
                shader_defs: defs.clone(),
                entry_point: "vertex".into(),
                buffers: Vec::new(),
            },
            fragment: Some(FragmentState {
                shader,
                shader_defs: defs,
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: if key.hdr {
                        bevy::render::view::ViewTarget::TEXTURE_FORMAT_HDR
                    } else {
                        TextureFormat::bevy_default()
                    },
                    blend: Some(blend),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                cull_mode: decal.then_some(Face::Front),
                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: if decal {
                    CompareFunction::Always
                } else {
                    CompareFunction::GreaterEqual
                },
                stencil: default(),
                bias: default(),
            }),
            multisample: MultisampleState {
                count: key.samples,
                ..default()
            },
            zero_initialize_workgroup_memory: false,
        }
    }
}

/// Queues the decal item and the card runs into the first 3D view with a depth prepass.
#[expect(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes system parameters by value, each a separate declared dependency"
)]
fn queue_effects(
    draw_functions: Res<'_, DrawFunctions<Transparent3d>>,
    pipeline: Res<'_, EffectPipeline>,
    mut pipelines: ResMut<'_, SpecializedRenderPipelines<EffectPipeline>>,
    cache: Res<'_, PipelineCache>,
    mut gpu: ResMut<'_, EffectGpu>,
    entities: Option<Res<'_, EffectRunEntities>>,
    mut runs: ResMut<'_, EffectRuns>,
    mut phases: ResMut<'_, ViewSortedRenderPhases<Transparent3d>>,
    views: Query<'_, '_, (Entity, &ExtractedView, &Msaa, &ViewPrepassTextures)>,
) {
    let Some(entities) = entities else {
        return;
    };
    let draw = draw_functions.read().id::<DrawEffects>();
    gpu.age_view = None;
    let Some((view_entity, view, msaa, _)) = views
        .iter()
        .find(|(_, _, _, prepass)| prepass.depth.is_some())
    else {
        return;
    };
    let Some(phase) = phases.get_mut(&view_entity) else {
        return;
    };
    gpu.age_view = Some(view_entity);
    let key = |kind, darken| EffectPipelineKey {
        kind,
        samples: msaa.samples(),
        hdr: view.hdr,
        darken,
    };
    // PERFORMANCE RATIONALE:
    // Why: A render pipeline is compiled when it is first asked for, and an item whose pipeline
    // is still compiling is not drawn. Asked for only once cards exist, the card and decal
    // pipelines compiled while the first effects of a session played, which then went undrawn
    // for most of a second (WP9 review 2, E2-1: a barrier staged up with its unit showed no
    // shell until about 0.9 s).
    // What: Every frame the four card and decal pipelines of the view are asked for, whether
    // or not anything is drawn, so they are compiled before the first effect.
    // Trade-off: Four lookups in the specialization cache per frame (hashes of a small key).
    for kind in [RunKind::Cards, RunKind::Decals] {
        for darken in [false, true] {
            pipelines.specialize(&cache, &pipeline, key(kind, darken));
        }
    }
    let placeholder = MainEntity::from(Entity::PLACEHOLDER);
    let mut ground = |pipeline, entity| {
        phase.add(Transparent3d {
            distance: f32::MIN,
            pipeline,
            entity: (entity, placeholder),
            draw_function: draw,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::NONE,
        });
    };
    // Every darkening element first, the darkening decals and then the darkening cards (all
    // ground-lying), so they multiply only the opaque scene; then the other decals over them.
    // The phase's sort is stable, so items of one distance keep the order they are added in
    // (WP8 review 3, E3-1: a darkening card queued at its run's distance multiplied whatever
    // transparent item sorted before it).
    // The decal buffer holds this frame's decals only once the aging pass can run (or the CPU
    // source wrote them).
    let decals = gpu.decal_count > 0
        && gpu.decal_group.is_some()
        && gpu.age_pipelines == AgePipelines::Ready;
    runs.decals = 0..gpu.decal_count;
    if decals && gpu.darkening_decals {
        ground(
            pipelines.specialize(&cache, &pipeline, key(RunKind::Decals, true)),
            entities.decal,
        );
    }
    runs.darkening = 0..gpu.darkening_cards;
    if gpu.darkening_cards > 0 && gpu.card_group.is_some() {
        ground(
            pipelines.specialize(&cache, &pipeline, key(RunKind::Cards, true)),
            entities.darkening_cards,
        );
    }
    if decals {
        ground(
            pipelines.specialize(&cache, &pipeline, key(RunKind::Decals, false)),
            entities.decal,
        );
    }
    if gpu.card_count == 0 || gpu.card_group.is_none() {
        return;
    }
    let card_pipeline = pipelines.specialize(&cache, &pipeline, key(RunKind::Cards, false));
    let runs = &mut *runs;
    runs.others.clear();
    runs.others
        .extend(phase.items.iter().map(|item| item.distance));
    runs.others.sort_unstable_by(f32::total_cmp);
    let rangefinder = view.rangefinder3d();
    // The darkening prefix is drawn by its own item; the ordinary runs are cut over the cards
    // after it, which are back to front (WP8 review 4, E4-1: cut with the prefix's distances,
    // every card behind a switching tank fell into one run queued at the tank's distance).
    let prefix = gpu.darkening_cards;
    cut_runs(
        gpu.centers
            .iter()
            .skip(prefix as usize)
            .map(|center| rangefinder.distance_translation(center)),
        prefix,
        &runs.others,
        &mut runs.cards,
    );
    for (index, range) in runs.cards.iter().enumerate() {
        let Some(&entity) = entities.cards.get(index) else {
            break;
        };
        let first = gpu
            .centers
            .get(range.start as usize)
            .copied()
            .unwrap_or_default();
        phase.add(Transparent3d {
            distance: rangefinder.distance_translation(&first),
            pipeline: card_pipeline,
            entity: (entity, placeholder),
            draw_function: draw,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::NONE,
        });
    }
}

/// Queues the distortion item into the transmissive phase of the first 3D view with a depth
/// prepass (the view the cards are drawn in).
#[expect(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "Bevy passes system parameters by value, each a separate declared dependency"
)]
fn queue_distortions(
    draw_functions: Res<'_, DrawFunctions<Transmissive3d>>,
    pipeline: Res<'_, EffectPipeline>,
    mut pipelines: ResMut<'_, SpecializedRenderPipelines<EffectPipeline>>,
    cache: Res<'_, PipelineCache>,
    gpu: Res<'_, EffectGpu>,
    entities: Option<Res<'_, EffectRunEntities>>,
    mut phases: ResMut<'_, ViewSortedRenderPhases<Transmissive3d>>,
    views: Query<'_, '_, (Entity, &ExtractedView, &Msaa, &ViewPrepassTextures)>,
) {
    let Some(entities) = entities else {
        return;
    };
    let Some((view_entity, view, msaa, _)) = views
        .iter()
        .find(|(_, _, _, prepass)| prepass.depth.is_some())
    else {
        return;
    };
    let key = EffectPipelineKey {
        kind: RunKind::Distortions,
        samples: msaa.samples(),
        hdr: view.hdr,
        darken: false,
    };
    // PERFORMANCE RATIONALE:
    // Why: A pipeline compiles on first use, and an item whose pipeline is still compiling is
    // not drawn: asked for only once distortions existed, the session's first heat haze or
    // shimmer went undrawn for most of a second (WP9 review 3, E3-1; the gap E2-1 closed for
    // the card and decal pipelines).
    // What: The distortion pipeline of the view is asked for every frame, whether or not a
    // distortion is drawn, as `queue_effects` asks for the card and decal pipelines.
    // Trade-off: One lookup in the specialization cache per frame.
    let specialized = pipelines.specialize(&cache, &pipeline, key);
    if gpu.distortion_count == 0 || gpu.distortion_group.is_none() {
        return;
    }
    let Some(phase) = phases.get_mut(&view_entity) else {
        return;
    };
    phase.add(Transmissive3d {
        distance: f32::MAX,
        pipeline: specialized,
        entity: (entities.distortion, MainEntity::from(Entity::PLACEHOLDER)),
        draw_function: draw_functions.read().id::<DrawEffects>(),
        batch_range: 0..1,
        extra_index: PhaseItemExtraIndex::NONE,
    });
}

/// Cuts cards at `distances` (ascending: back to front) into runs that no other item's
/// distance in `others` (sorted ascending) falls inside; at most [`MAX_CARD_RUNS`] runs, the
/// last taking the rest. The first distance is card `start`'s, so the ranges written into
/// `runs` index the card buffer from there: the queue cuts only the cards after the darkening
/// prefix, whose own distances are not in back-to-front order with them (WP8 review 4, E4-1).
fn cut_runs(
    distances: impl Iterator<Item = f32>,
    start: u32,
    others: &[f32],
    runs: &mut Vec<Range<u32>>,
) {
    runs.clear();
    let mut below = 0_usize;
    for (offset, distance) in distances.enumerate() {
        let index = u32::try_from(offset)
            .unwrap_or(u32::MAX)
            .saturating_add(start);
        let mut crossed = false;
        while below < others.len() && others[below] < distance {
            below += 1;
            crossed = true;
        }
        let full = runs.len() == MAX_CARD_RUNS;
        match runs.last_mut() {
            Some(run) if !crossed || full => run.end = index + 1,
            _ => runs.push(index..index + 1),
        }
    }
}

/// The per-view bind group of the effect pipelines.
#[derive(Component)]
struct EffectViewBindGroup(BindGroup);

/// The per-view bind group of the distortion pipeline.
#[derive(Component)]
struct EffectDistortionViewBindGroup(BindGroup);

/// Builds each view's group from its uniforms and prepass depth.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn prepare_effect_view_bind_groups(
    mut commands: Commands<'_, '_>,
    pipeline: Res<'_, EffectPipeline>,
    device: Res<'_, RenderDevice>,
    uniforms: Res<'_, ViewUniforms>,
    gpu: Res<'_, EffectGpu>,
    views: Query<
        '_,
        '_,
        (
            Entity,
            &Msaa,
            &ViewPrepassTextures,
            Option<&ViewTransmissionTexture>,
        ),
        With<ExtractedView>,
    >,
) {
    if gpu.card_count == 0 && gpu.decal_count == 0 && gpu.distortion_count == 0 {
        return;
    }
    let Some(binding) = uniforms.uniforms.binding() else {
        return;
    };
    for (entity, msaa, prepass, transmission) in &views {
        if let Some(transmission) = transmission {
            let group = device.create_bind_group(
                "effect_distortion_view",
                &pipeline.distortion_view_layout,
                &BindGroupEntries::sequential((
                    binding.clone(),
                    &transmission.view,
                    &transmission.sampler,
                )),
            );
            commands
                .entity(entity)
                .insert(EffectDistortionViewBindGroup(group));
        }
        let Some(depth) = prepass.depth_view() else {
            continue;
        };
        let layout = if msaa.samples() > 1 {
            &pipeline.view_layout_msaa
        } else {
            &pipeline.view_layout
        };
        let group = device.create_bind_group(
            "effect_view",
            layout,
            &BindGroupEntries::sequential((binding.clone(), depth)),
        );
        commands.entity(entity).insert(EffectViewBindGroup(group));
    }
}

/// The draw of one effect phase item.
type DrawEffects = (SetItemPipeline, SetEffectBindGroups, DrawEffectRun);

/// Binds the view group (with its dynamic offset) and the block's resource group.
struct SetEffectBindGroups;

impl<P: PhaseItem> RenderCommand<P> for SetEffectBindGroups {
    type Param = SRes<EffectGpu>;
    type ViewQuery = (
        Read<ViewUniformOffset>,
        Option<Read<EffectViewBindGroup>>,
        Option<Read<EffectDistortionViewBindGroup>>,
    );
    type ItemQuery = Read<EffectRunSlot>;

    fn render<'w>(
        _item: &P,
        (offset, view_group, distortion_group): ROQueryItem<'w, Self::ViewQuery>,
        slot: Option<ROQueryItem<'w, Self::ItemQuery>>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(slot) = slot else {
            return RenderCommandResult::Skip;
        };
        let view_group = match slot.kind {
            RunKind::Distortions => distortion_group.map(|group| &group.0),
            RunKind::Cards | RunKind::Decals => view_group.map(|group| &group.0),
        };
        let Some(view_group) = view_group else {
            return RenderCommandResult::Skip;
        };
        let gpu = gpu.into_inner();
        let group = match slot.kind {
            RunKind::Cards => &gpu.card_group,
            RunKind::Decals => &gpu.decal_group,
            RunKind::Distortions => &gpu.distortion_group,
        };
        let Some(group) = group else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(0, view_group, &[offset.offset]);
        pass.set_bind_group(1, group, &[]);
        RenderCommandResult::Success
    }
}

/// Draws one run's instance range.
struct DrawEffectRun;

impl<P: PhaseItem> RenderCommand<P> for DrawEffectRun {
    type Param = (SRes<EffectRuns>, SRes<EffectGpu>);
    type ViewQuery = ();
    type ItemQuery = Read<EffectRunSlot>;

    fn render<'w>(
        _item: &P,
        (): ROQueryItem<'w, Self::ViewQuery>,
        slot: Option<ROQueryItem<'w, Self::ItemQuery>>,
        (runs, gpu): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(slot) = slot else {
            return RenderCommandResult::Skip;
        };
        let runs = runs.into_inner();
        match slot.kind {
            RunKind::Decals => pass.draw(0..DECAL_VERTICES, runs.decals.clone()),
            RunKind::Distortions => {
                pass.draw(0..CARD_VERTICES, 0..gpu.into_inner().distortion_count);
            }
            RunKind::Cards if slot.index == DARKENING_RUN => {
                pass.draw(0..CARD_VERTICES, runs.darkening.clone());
            }
            RunKind::Cards => {
                let Some(range) = runs.cards.get(slot.index as usize) else {
                    return RenderCommandResult::Skip;
                };
                pass.draw(0..CARD_VERTICES, range.clone());
            }
        }
        RenderCommandResult::Success
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a failed test is a panic")]
mod tests {
    use super::*;
    use crate::effects::test_streams::{run_stream_with, StreamKind};

    /// D-11: the render world's staged patches stay in the vector made at the decal budget's
    /// capacity over both streams, with a full copy every 61st frame (the sync harness has no
    /// render world, so this is the render half of `sync_steady_state_tests`).
    #[test]
    fn staged_patches_never_outgrow_their_retained_vector() {
        for kind in [StreamKind::Splashes, StreamKind::Plan] {
            let mut staging: Option<Vec<GpuDecalPatch>> = None;
            let mut buffer = None;
            let (mut frame, mut staged) = (0_u32, 0_usize);
            run_stream_with(
                kind,
                600,
                |_| {},
                |catalog, runtime, _| {
                    let staging = staging.get_or_insert_with(|| {
                        Vec::with_capacity(catalog.budget.decals.max(1) as usize)
                    });
                    frame += 1;
                    let table = runtime.decal_table();
                    let copy = if frame % 61 == 0 {
                        RowCopy::Every
                    } else {
                        table.copy_since(Some(table.generation() - 1))
                    };
                    assert_eq!(
                        stage_patches(table, copy, staging),
                        0,
                        "every row has a record"
                    );
                    staged += staging.len();
                    let now = (staging.as_ptr() as usize, staging.capacity());
                    assert_eq!(*buffer.get_or_insert(now), now, "{kind:?}: no growth");
                },
            );
            assert!(staged > 0, "{kind:?}: rows were staged");
        }
    }

    fn cut(distances: &[f32], others: &[f32]) -> Vec<Range<u32>> {
        let mut runs = Vec::new();
        cut_runs(distances.iter().copied(), 0, others, &mut runs);
        runs
    }

    /// The ordinary runs of a card buffer whose first `prefix` cards darken, cut as the queue
    /// cuts them.
    fn cut_after(prefix: u32, distances: &[f32], others: &[f32]) -> Vec<Range<u32>> {
        let mut runs = Vec::new();
        cut_runs(
            distances.iter().copied().skip(prefix as usize),
            prefix,
            others,
            &mut runs,
        );
        runs
    }

    /// WP8 review 4, E4-1: a darkening prefix nearer than every other card neither joins an
    /// ordinary run nor moves its cuts.
    #[test]
    fn the_darkening_prefix_stays_out_of_the_ordinary_runs() {
        let others = [-9.5, -8.5];
        let ordinary = [-10.0, -9.0, -8.0];
        let with_prefix = [-1.0, -10.0, -9.0, -8.0];
        let runs = cut_after(1, &with_prefix, &others);
        assert!(
            runs.iter().all(|run| !run.contains(&0)),
            "no ordinary run draws the darkening card: {runs:?}"
        );
        let shifted: Vec<Range<u32>> = cut(&ordinary, &others)
            .into_iter()
            .map(|run| run.start + 1..run.end + 1)
            .collect();
        assert_eq!(runs, shifted, "cut at -9.5 and -8.5 as without the prefix");
        assert_eq!(runs, vec![1..2, 2..3, 3..4]);
        // The defect it replaces: cut with the prefix, the near darkening card moved the cursor
        // past every other item, and the cards behind it fell into its run.
        assert_eq!(cut(&with_prefix, &others), vec![0..4]);
    }

    #[test]
    fn cards_with_no_item_between_them_are_one_run() {
        assert_eq!(cut(&[-9.0, -8.0, -7.0], &[]), vec![0..3]);
        assert_eq!(cut(&[-9.0, -8.0], &[-20.0, -1.0]), vec![0..2]);
        assert_eq!(cut(&[], &[-5.0]), Vec::<Range<u32>>::new());
    }

    #[test]
    fn a_bubble_between_cards_splits_them() {
        // A bubble at -6 sits between the far cards (-9, -8) and the near ones (-4, -3).
        assert_eq!(cut(&[-9.0, -8.0, -4.0, -3.0], &[-6.0]), vec![0..2, 2..4]);
        assert_eq!(
            cut(&[-9.0, -5.0, -1.0], &[-10.0, -7.0, -6.0, -2.0]),
            vec![0..1, 1..2, 2..3]
        );
    }

    #[test]
    fn runs_past_the_cap_merge_into_the_last() {
        let distances: Vec<f32> = (0..200_u16).map(f32::from).collect();
        let others: Vec<f32> = (0..200_u16).map(|i| f32::from(i) - 0.5).collect();
        let runs = cut(&distances, &others);
        assert_eq!(runs.len(), MAX_CARD_RUNS);
        assert_eq!(runs.last().map(|run| run.end), Some(200));
    }
}
