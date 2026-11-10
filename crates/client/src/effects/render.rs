//! The instanced effect renderer: retained GPU buffers, two pipelines and per-run draws in the
//! transparent phase (plan §5.1, §5.6 "Sort and blend").
//!
//! # Responsibility
//! - **Buffers.** One storage buffer of [`SpriteInstance`] (sprites, rings and particles) and
//!   one of [`DecalInstance`], created once at the budget's capacity when the vocabulary is
//!   loaded and written in place during extraction (`RenderQueue::write_buffer`), as
//!   `vfx::gpu_upload` writes its slots. No asset is created or modified per frame.
//! - **Textures.** The drawable texel array of every listed texture set
//!   (`texture_2d_array`, mips included) and the ramp atlas (one 256-texel `Rgba32Float` row
//!   per ramp, built from data), created once per vocabulary.
//! - **Pipelines.** Cards: a camera-, ground- or line-oriented quad per instance, premultiplied
//!   blending (`ONE, ONE_MINUS_SRC_ALPHA`, so additive parts with alpha 0 share the pass),
//!   depth test against the opaque depth, no depth write, soft depth against the depth
//!   prepass. Decals: a box per instance drawn by its back faces with no depth test, the
//!   ground point reconstructed from the depth prepass and tested against the box.
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
//! - Runs are queued for the first 3D view with a depth prepass only.

use super::catalog::EffectCatalog;
use super::curve::RAMP_TEXELS;
use super::instances::{DecalInstance, SpriteInstance};
use super::runtime::EffectRuntime;
use bevy::core_pipeline::core_3d::{Transparent3d, CORE_3D_DEPTH_FORMAT};
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::ecs::query::ROQueryItem;
use bevy::ecs::system::lifetimeless::{Read, SRes};
use bevy::ecs::system::SystemParamItem;
use bevy::image::BevyDefault;
use bevy::prelude::*;
use bevy::render::render_phase::{
    sort_phase_system, AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
    RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
};
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only_sized, texture_2d, texture_2d_array, texture_depth_2d,
    texture_depth_2d_multisampled, uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BlendComponent,
    BlendFactor, BlendOperation, BlendState, Buffer, BufferDescriptor, BufferUsages,
    ColorTargetState, ColorWrites, CompareFunction, DepthStencilState, Extent3d, Face,
    FragmentState, MultisampleState, PipelineCache, PrimitiveState, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, SpecializedRenderPipeline,
    SpecializedRenderPipelines, TextureDataOrder, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexState,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSet};
use std::ops::Range;

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
/// Shared import of both.
const COMMON_SHADER: &str = "shaders/effects/common.wgsl";

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
            .add_systems(ExtractSchedule, extract_effects)
            .add_systems(
                Render,
                (
                    queue_effects
                        .in_set(RenderSet::PhaseSort)
                        .before(sort_phase_system::<Transparent3d>),
                    prepare_effect_view_bind_groups.in_set(RenderSet::PrepareBindGroups),
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.init_resource::<EffectPipeline>();
        let world = render_app.world_mut();
        let cards = (0..MAX_CARD_RUNS)
            .map(|index| {
                world
                    .spawn(EffectRunSlot {
                        decal: false,
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                    })
                    .id()
            })
            .collect();
        let decal = world
            .spawn(EffectRunSlot {
                decal: true,
                index: 0,
            })
            .id();
        world.insert_resource(EffectRunEntities { cards, decal });
        let gpu = EffectGpu::new(world.resource::<RenderDevice>());
        world.insert_resource(gpu);
    }
}

/// Which run a phase item draws.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct EffectRunSlot {
    /// Whether it is the decal item.
    decal: bool,
    /// Index of the card run.
    index: u32,
}

/// The render-world entities phase items point at.
#[derive(Resource, Debug)]
struct EffectRunEntities {
    /// One per card run.
    cards: Vec<Entity>,
    /// The decal item.
    decal: Entity,
}

/// This frame's instance ranges per run.
#[derive(Resource, Debug, Default)]
struct EffectRuns {
    /// Card ranges, indexed by [`EffectRunSlot::index`].
    cards: Vec<Range<u32>>,
    /// The decal range.
    decals: Range<u32>,
    /// Distances of the view's other transparent items, sorted (retained scratch).
    others: Vec<f32>,
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
    /// Bind group of the card pipeline's resources.
    card_group: Option<BindGroup>,
    /// Bind group of the decal pipeline's resources.
    decal_group: Option<BindGroup>,
    /// Shared sampler.
    sampler: Sampler,
    /// Cards to draw this frame.
    card_count: u32,
    /// Decals to draw this frame.
    decal_count: u32,
    /// Card centres in draw order (retained), for the runs.
    centers: Vec<Vec3>,
}

impl EffectGpu {
    /// Empty resources with the shared sampler.
    fn new(device: &RenderDevice) -> Self {
        Self {
            version: None,
            cards: None,
            decals: None,
            card_group: None,
            decal_group: None,
            sampler: device.create_sampler(&SamplerDescriptor {
                label: Some("effect_sampler"),
                mag_filter: bevy::render::render_resource::FilterMode::Linear,
                min_filter: bevy::render::render_resource::FilterMode::Linear,
                mipmap_filter: bevy::render::render_resource::FilterMode::Linear,
                ..default()
            }),
            card_count: 0,
            decal_count: 0,
            centers: Vec::new(),
        }
    }

    /// Builds buffers, textures and bind groups for `catalog`.
    fn rebuild(
        &mut self,
        catalog: &EffectCatalog,
        device: &RenderDevice,
        queue: &RenderQueue,
        layout: &BindGroupLayout,
    ) {
        self.card_group = None;
        self.decal_group = None;
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
        let decals = buffer(
            "effect_decals",
            decal_capacity * size_of::<DecalInstance>() as u64,
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
        }
        self.cards = Some(cards);
        self.decals = Some(decals);
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

/// Copies this frame's cards and decals into the retained buffers.
// PERFORMANCE RATIONALE:
// Why: Effects change every frame; recreating buffers or touching assets per frame would
// allocate GPU memory and bind groups continuously (the vfx C-17 finding).
// What: The buffers are made once per vocabulary at the budget's capacity; each frame writes
// only the live prefix with one `write_buffer` per block and copies the card centres into a
// retained vector.
// Trade-off: The capacity is reserved on the GPU even when few effects are live.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn extract_effects(
    catalog: Extract<'_, '_, Option<Res<'_, EffectCatalog>>>,
    version: Extract<'_, '_, Option<Res<'_, EffectCatalogVersion>>>,
    runtime: Extract<'_, '_, Option<Res<'_, EffectRuntime>>>,
    mut gpu: ResMut<'_, EffectGpu>,
    pipeline: Res<'_, EffectPipeline>,
    device: Res<'_, RenderDevice>,
    queue: Res<'_, RenderQueue>,
) {
    let (Some(catalog), Some(version), Some(runtime)) = (&*catalog, &*version, &*runtime) else {
        gpu.card_count = 0;
        gpu.decal_count = 0;
        return;
    };
    if gpu.version != Some(version.0) {
        gpu.rebuild(catalog, &device, &queue, &pipeline.resource_layout);
        gpu.version = Some(version.0);
    }
    let frame = &runtime.frame;
    let gpu = &mut *gpu;
    gpu.card_count = u32::try_from(frame.sorted.len()).unwrap_or(0);
    gpu.decal_count = u32::try_from(frame.decals.len()).unwrap_or(0);
    if !runtime.is_changed() {
        return;
    }
    if let (Some(buffer), false) = (&gpu.cards, frame.sorted.is_empty()) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&frame.sorted));
    }
    if let (Some(buffer), false) = (&gpu.decals, frame.decals.is_empty()) {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&frame.decals));
    }
    gpu.centers.clear();
    gpu.centers.extend(
        frame
            .sorted
            .iter()
            .map(|card| Vec3::from_array(card.center)),
    );
}

/// The two effect pipelines' shared layouts and shaders.
#[derive(Resource)]
struct EffectPipeline {
    /// View group: uniforms and the prepass depth, single-sampled.
    view_layout: BindGroupLayout,
    /// View group with a multisampled prepass depth.
    view_layout_msaa: BindGroupLayout,
    /// Resource group: instances, atlas, sampler, ramps.
    resource_layout: BindGroupLayout,
    /// Card shader.
    card_shader: Handle<Shader>,
    /// Decal shader.
    decal_shader: Handle<Shader>,
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
            resource_layout,
            card_shader: assets.load(CARD_SHADER),
            decal_shader: assets.load(DECAL_SHADER),
            _common_shader: assets.load(COMMON_SHADER),
        }
    }
}

/// What a specialised effect pipeline depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EffectPipelineKey {
    /// Decal pipeline rather than cards.
    decal: bool,
    /// MSAA sample count of the view.
    samples: u32,
    /// Whether the view renders to an HDR target.
    hdr: bool,
}

impl SpecializedRenderPipeline for EffectPipeline {
    type Key = EffectPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let shader = if key.decal {
            self.decal_shader.clone()
        } else {
            self.card_shader.clone()
        };
        let mut defs = Vec::new();
        if key.samples > 1 {
            defs.push("MULTISAMPLED".into());
        }
        let view_layout = if key.samples > 1 {
            self.view_layout_msaa.clone()
        } else {
            self.view_layout.clone()
        };
        let premultiplied = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        };
        RenderPipelineDescriptor {
            label: Some(if key.decal {
                "effect_decals".into()
            } else {
                "effect_cards".into()
            }),
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
                    blend: Some(BlendState {
                        color: premultiplied,
                        alpha: premultiplied,
                    }),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                cull_mode: key.decal.then_some(Face::Front),
                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: if key.decal {
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
    gpu: Res<'_, EffectGpu>,
    entities: Option<Res<'_, EffectRunEntities>>,
    mut runs: ResMut<'_, EffectRuns>,
    mut phases: ResMut<'_, ViewSortedRenderPhases<Transparent3d>>,
    views: Query<'_, '_, (Entity, &ExtractedView, &Msaa, &ViewPrepassTextures)>,
) {
    let Some(entities) = entities else {
        return;
    };
    let draw = draw_functions.read().id::<DrawEffects>();
    let Some((view_entity, view, msaa, _)) = views
        .iter()
        .find(|(_, _, _, prepass)| prepass.depth.is_some())
    else {
        return;
    };
    let Some(phase) = phases.get_mut(&view_entity) else {
        return;
    };
    let key = |decal| EffectPipelineKey {
        decal,
        samples: msaa.samples(),
        hdr: view.hdr,
    };
    let placeholder = MainEntity::from(Entity::PLACEHOLDER);
    if gpu.decal_count > 0 && gpu.decal_group.is_some() {
        runs.decals = 0..gpu.decal_count;
        phase.add(Transparent3d {
            distance: f32::MIN,
            pipeline: pipelines.specialize(&cache, &pipeline, key(true)),
            entity: (entities.decal, placeholder),
            draw_function: draw,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::NONE,
        });
    }
    if gpu.card_count == 0 || gpu.card_group.is_none() {
        return;
    }
    let card_pipeline = pipelines.specialize(&cache, &pipeline, key(false));
    let runs = &mut *runs;
    runs.others.clear();
    runs.others
        .extend(phase.items.iter().map(|item| item.distance));
    runs.others.sort_unstable_by(f32::total_cmp);
    let rangefinder = view.rangefinder3d();
    cut_runs(
        gpu.centers
            .iter()
            .map(|center| rangefinder.distance_translation(center)),
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

/// Cuts cards at `distances` (ascending: back to front) into runs that no other item's
/// distance in `others` (sorted ascending) falls inside; at most [`MAX_CARD_RUNS`] runs, the
/// last taking the rest. Writes the instance ranges into `runs`.
fn cut_runs(distances: impl Iterator<Item = f32>, others: &[f32], runs: &mut Vec<Range<u32>>) {
    runs.clear();
    let mut below = 0_usize;
    for (index, distance) in distances.enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
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
    views: Query<'_, '_, (Entity, &Msaa, &ViewPrepassTextures), With<ExtractedView>>,
) {
    if gpu.card_count == 0 && gpu.decal_count == 0 {
        return;
    }
    let Some(binding) = uniforms.uniforms.binding() else {
        return;
    };
    for (entity, msaa, prepass) in &views {
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
    type ViewQuery = (Read<ViewUniformOffset>, Option<Read<EffectViewBindGroup>>);
    type ItemQuery = Read<EffectRunSlot>;

    fn render<'w>(
        _item: &P,
        (offset, view_group): ROQueryItem<'w, Self::ViewQuery>,
        slot: Option<ROQueryItem<'w, Self::ItemQuery>>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (Some(view_group), Some(slot)) = (view_group, slot) else {
            return RenderCommandResult::Skip;
        };
        let gpu = gpu.into_inner();
        let group = if slot.decal {
            &gpu.decal_group
        } else {
            &gpu.card_group
        };
        let Some(group) = group else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(0, &view_group.0, &[offset.offset]);
        pass.set_bind_group(1, group, &[]);
        RenderCommandResult::Success
    }
}

/// Draws one run's instance range.
struct DrawEffectRun;

impl<P: PhaseItem> RenderCommand<P> for DrawEffectRun {
    type Param = SRes<EffectRuns>;
    type ViewQuery = ();
    type ItemQuery = Read<EffectRunSlot>;

    fn render<'w>(
        _item: &P,
        (): ROQueryItem<'w, Self::ViewQuery>,
        slot: Option<ROQueryItem<'w, Self::ItemQuery>>,
        runs: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(slot) = slot else {
            return RenderCommandResult::Skip;
        };
        let runs = runs.into_inner();
        if slot.decal {
            pass.draw(0..DECAL_VERTICES, runs.decals.clone());
        } else if let Some(range) = runs.cards.get(slot.index as usize) {
            pass.draw(0..CARD_VERTICES, range.clone());
        } else {
            return RenderCommandResult::Skip;
        }
        RenderCommandResult::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(distances: &[f32], others: &[f32]) -> Vec<Range<u32>> {
        let mut runs = Vec::new();
        cut_runs(distances.iter().copied(), others, &mut runs);
        runs
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
