//! Custom Bevy Material and GPU uniform binding for Unit Bubbles and Building Domes.
//!
//! # Architectural Responsibilities
//! This module binds CPU simulation snapshots into GPU-visible `BubbleUniformGpu` structures
//! and manages the `BubbleDomeMaterial` shader pipeline. It translates vertical strip flipbooks
//! into high-performance `Texture2DArray` assets and samples active frames with zero heap allocations.
//!
//! The flipbook layer is chosen on the GPU from the shared [`FlipbookClock`](crate::flipbook_clock)
//! and the bubble's [`FlipbookLanes`], which change only when the bubble's action changes, so time
//! alone never modifies a material.
//!
//! # Invariants
//! - `BubbleUniformGpu` memory layout is strictly 176 bytes and 16-byte aligned matching `bubble_dome.wgsl`.
//! - Every `BubbleDomeMaterial` binds the shared clock buffer at binding 3: [`BubbleDomeMaterial::new`]
//!   is the only constructor, and [`BubbleDomeMaterialPlugin`] registers the material with its clock.
//! - Mobile units and building domes share one full UV sphere mesh. A dome is placed with its centre
//!   on the ground, so its lower half lies below the terrain and only the upper half is seen.
//! - Missing or malformed media is retained as typed unavailable status; no texture is substituted.

use asset_baker::manifest::FlipbookAtlasMeta;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use bevy::render::storage::ShaderStorageBuffer;
use sim_types::enums::{AnimState, ShapeKind};
use sim_types::ids::UnitTypeId;
use sim_types::snapshot::BubbleRenderInstance;
use std::collections::BTreeMap;

use crate::flipbook_clock::{FlipbookClockPlugin, FlipbookLanes, FLIPBOOK_CLOCK_BUFFER};

/// ECS Component wrapping authoritative [`BubbleRenderInstance`] snapshot data.
#[derive(Component, Clone, Debug, Deref, DerefMut)]
pub struct BubbleInstance(pub BubbleRenderInstance);

/// Feature bitflag: Cloaking shimmer interference.
pub const FLAG_CLOAKED: u32 = 1 << 0;
/// Feature bitflag: Tactical selection outer rim glow.
pub const FLAG_SELECTED: u32 = 1 << 1;
/// Feature bitflag: Crisp dual-light studio specular glare.
pub const FLAG_GLARE: u32 = 1 << 2;
/// Feature bitflag: Thin-film soap-bubble iridescent Fresnel rim.
pub const FLAG_FRESNEL: u32 = 1 << 3;
/// Feature bitflag: View-aligned spherical inner video projection with barrel lens warp.
pub const FLAG_INNER_WARP: u32 = 1 << 4;
/// Feature bitflag: Voronoi spiderweb glass cracks & electrical arcing.
pub const FLAG_CRACKS: u32 = 1 << 5;
/// Feature bitflag: CRT scanlines & chromatic aberration jitter.
pub const FLAG_CRT_GLITCH: u32 = 1 << 6;
/// Feature bitflag: Pulsing rim arc trailing a speed-boosted unit.
pub const FLAG_SPEED_BOOSTED: u32 = 1 << 7;
/// Feature bitflag: Direct-on-sphere holographic facing stripe & nose chevron.
pub const FLAG_FACING_STRIPE: u32 = 1 << 8;
/// Feature bitflag: Attack target-lock aperture ring on sphere.
pub const FLAG_ATTACK_APERTURE: u32 = 1 << 9;
/// Feature bitflag: Blend consecutive flipbook layers of a loop by the sub-frame time
/// (plan D-6). A bubble blends only when its `anim_mode` also carries
/// [`ANIM_BLEND_BIT`](crate::flipbook_clock::ANIM_BLEND_BIT).
pub const FLAG_FRAME_BLEND: u32 = 1 << 10;

/// Default active optical features.
pub const DEFAULT_FEATURE_FLAGS: u32 = FLAG_GLARE
    | FLAG_FRESNEL
    | FLAG_INNER_WARP
    | FLAG_CRACKS
    | FLAG_CRT_GLITCH
    | FLAG_FACING_STRIPE
    | FLAG_ATTACK_APERTURE
    | FLAG_FRAME_BLEND;

mod gpu_layout {
    //! Holds one `ShaderType` struct, so the lint expectation below covers nothing else.
    #![expect(
        dead_code,
        reason = "the ShaderType derive emits layout checks nothing calls"
    )]

    use bevy::prelude::*;
    use bevy::render::render_resource::ShaderType;

    /// GPU uniform buffer struct matching `struct BubbleUniform` in `bubble_dome.wgsl`.
    // PERFORMANCE RATIONALE:
    // Why: WebGPU uniform buffers require strict 16-byte alignment and explicit std140 padding.
    // What: Fields are strictly ordered: vec4 (16B), vec3+f32 (16B), 4x u32/f32 (16B) chunks, totaling exactly 176 bytes.
    // Modular optical feature toggles are packed into `feature_flags: u32` bitmask with four additional optical vec4 lanes after the original 96 bytes,
    // and the flipbook lanes that do not fit the earlier rows fill the last 16-byte row.
    // Trade-off: Manual struct layout in exchange for zero runtime padding overhead and zero shader misalignment faults.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
    pub struct BubbleUniformGpu {
        /// Team Fresnel rim and ground indicator color (RGBA).
        pub team_color: Vec4,
        /// Authoritative 3D center position [x, `ground_y`, `altitude_z`].
        pub world_pos: Vec3,
        /// Sphere collision radius in world units.
        pub radius: f32,
        /// 0 = `UnitSphere` (mobile), 1 = `BuildingDome` (grounded structure).
        pub shape_kind: u32,
        /// Facing orientation angle in radians.
        pub heading_angle_rad: f32,
        /// Independent weapon turret aim angle in radians.
        pub aim_angle_rad: f32,
        /// Active firing / windup state flag (1 = firing, 0 = idle).
        pub is_attacking: u32,
        /// High-level animation action state index.
        pub anim_state: u32,
        /// Flipbook clock time at which the bubble's elapsed animation time is zero, in clock
        /// units of 1/1024 tick (`FlipbookLanes::origin`).
        pub anim_origin: u32,
        /// Health ratio [0.0, 1.0] (cracks appear < 0.75, critical glitch < 0.35).
        pub hp_ratio: f32,
        /// Shield energy ratio [0.0, 1.0].
        pub shield_ratio: f32,
        /// Decaying shield ripple flash intensity [0.0, 1.0].
        pub shield_hit_flash: f32,
        /// Direction the last hull hit originated from in radians.
        pub hull_hit_angle_rad: f32,
        /// Decaying hull impact flare intensity [0.0, 1.0].
        pub hull_hit_flash: f32,
        /// Built share of the shell's height [0.0, 1.0]: 0 at the base, 1 at the top. Read only while
        /// `anim_state` is `AnimState::Constructing`, so a production queue never paints the shell.
        pub build_progress: f32,
        /// Selection highlight flag (1 = selected, 0 = unselected).
        pub selected: u32,
        /// Stealth cloaking flag (1 = cloaked shimmer, 0 = opaque).
        pub cloaked: u32,
        /// Modular feature bitflags bitmask (glare, fresnel, inner warp, cracks, scanlines).
        pub feature_flags: u32,
        /// First `Texture2DArray` layer of the playing slice, or the layer shown in fixed mode
        /// (`FlipbookLanes::base`).
        pub frame_base: u32,
        /// Media exposure stops, dielectric reflectance, Fresnel power, reflection strength.
        pub optics_media: Vec4,
        /// Glare roughness/strength, hit strength, hull hit angular width in radians.
        pub optics_glare: Vec4,
        /// Selection strength, shell alpha, facing strength, attack strength.
        pub optics_signals: Vec4,
        /// Direction toward the main light in world space; W is normalized illuminance.
        pub optics_light: Vec4,
        /// Clock units of one pass over the slice (`FlipbookLanes::period`).
        pub anim_period: u32,
        /// Layers in the slice (`FlipbookLanes::count`).
        pub frame_count: u32,
        /// Flipbook mode in bits 0 and 1, blend request in bit 2 (`FlipbookLanes::mode`).
        pub anim_mode: u32,
        /// Loop phase offset in clock units (`FlipbookLanes::offset`).
        pub anim_offset: u32,
    }

    /// GPU uniform matching `struct ShellUniform` in `bubble_dome.wgsl`: the modulation of a
    /// bubble by the effects held on its unit (plan `2026-11-09-effect-visualization` §5.1
    /// `Shell`, WP8).
    // PERFORMANCE RATIONALE:
    // Why: The shell lanes change only when a unit's stance or concealment look changes, while
    // `BubbleUniformGpu` is compared and rewritten for every bubble; and the smog goldens hash
    // that uniform's bytes, which must not change for bubbles no effect touches.
    // What: A separate 64-byte uniform at binding 4, four 16-byte rows, neutral (all zero)
    // unless an effect is held on the unit.
    // Trade-off: One more binding per bubble material, in exchange for an unchanged main
    // uniform and no rewrite of it when only a shell lane moves.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
    pub struct ShellUniformGpu {
        /// Linear RGB tint of the media and the share it replaces (0 = none).
        pub tint: Vec4,
        /// Linear RGB of the light along the outline and its strength (0 = none).
        pub rim: Vec4,
        /// World units the bubble is sunk below where it rests; share of it made see-through;
        /// height in world units of the soil stain above `ground` (its fourth lane is the
        /// world height of the ground under the unit). All zero leaves the bubble as it is.
        pub shape: Vec4,
        /// 1 when effects drive this bubble's concealment look (the legacy cloak shimmer of
        /// `cloaked` is then off), else 0; the other lanes are padding.
        pub driven: Vec4,
    }
}
pub use gpu_layout::{BubbleUniformGpu, ShellUniformGpu};

impl Default for ShellUniformGpu {
    fn default() -> Self {
        Self {
            tint: Vec4::ZERO,
            rim: Vec4::ZERO,
            shape: Vec4::ZERO,
            driven: Vec4::ZERO,
        }
    }
}

impl Default for BubbleUniformGpu {
    fn default() -> Self {
        Self {
            team_color: Vec4::ZERO,
            world_pos: Vec3::ZERO,
            radius: 0.5,
            shape_kind: 0,
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: 0,
            anim_state: 1, // Idle
            anim_origin: 0,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_progress: 1.0,
            selected: 0,
            cloaked: 0,
            feature_flags: DEFAULT_FEATURE_FLAGS,
            frame_base: 0,
            optics_media: crate::bubble_optics::BubbleOptics::default().uniform_lanes()[0],
            optics_glare: crate::bubble_optics::BubbleOptics::default().uniform_lanes()[1],
            optics_signals: crate::bubble_optics::BubbleOptics::default().uniform_lanes()[2],
            optics_light: Vec4::new(0.0, 1.0, 0.0, 0.0),
            anim_period: 0,
            frame_count: 0,
            anim_mode: 0,
            anim_offset: 0,
        }
    }
}

/// Updates a bubble material only when the change would be visible.
///
/// Returns `true` when the asset was modified.
// PERFORMANCE RATIONALE:
// Why: `Assets::get_mut` emits `AssetEvent::Modified`, and Bevy then rebuilds that material's uniform
// buffer and bind group. Unconditional writes did this for every unit on every frame.
// What: An immutable lookup compares first, so only a visible change reaches `get_mut`.
// Trade-off: Two lookups for a changed material, in exchange for none of that work when unchanged.
pub fn sync_bubble_material(
    materials: &mut Assets<BubbleDomeMaterial>,
    handle: &Handle<BubbleDomeMaterial>,
    uniform: &BubbleUniformGpu,
    texture: Option<Handle<Image>>,
) -> bool {
    let unchanged = materials.get(handle).is_none_or(|material| {
        material.uniform.renders_same_as(uniform) && material.video_texture == texture
    });
    if unchanged {
        return false;
    }
    let Some(material) = materials.get_mut(handle) else {
        return false;
    };
    material.uniform.apply_snapshot(uniform);
    material.video_texture = texture;
    true
}

/// Atlas layer that shows `anim_state` at `progress` (`0.0..1.0` of one cycle of its slice).
///
/// With `meta`, the action's slice is used: a looping slice wraps, a one-shot holds its last
/// frame, and an action without a slice shows layer 0. Without `meta`, the legacy layout of
/// eight frames per action is assumed. Only still pictures use it ([`still_frame_lanes`]); every
/// animated bubble plays its slice on the flipbook clock
/// ([`battle_uniform`](crate::flipbook_clock::battle_uniform),
/// [`scene_lanes`](crate::flipbook_clock::scene_lanes)).
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation values are bounded by frame, atlas and window sizes"
)]
pub fn frame_index_at(
    meta: Option<&FlipbookAtlasMeta>,
    anim_state: AnimState,
    progress: f32,
) -> u32 {
    let Some(meta) = meta else {
        let state_offset = match anim_state {
            AnimState::Constructing => 0,
            AnimState::Idle => 8,
            AnimState::Walk => 16,
            AnimState::Attack | AnimState::Cast => 24,
            AnimState::Die => 32,
        };
        return state_offset + ((progress * 8.0) as u32 % 8);
    };
    let Some(slice) = meta.actions.get(&anim_state) else {
        return 0;
    };
    let step = (progress * slice.frame_count as f32) as u32;
    if slice.frame_count == 0 {
        slice.start_frame
    } else if slice.looping {
        slice.start_frame + (step % slice.frame_count)
    } else {
        slice.start_frame + step.min(slice.frame_count.saturating_sub(1))
    }
}

/// Lanes that hold the first frame of `anim_state`'s slice (see [`frame_index_at`] at progress 0),
/// the picture the swarm preview shows.
#[must_use]
pub fn still_frame_lanes(meta: Option<&FlipbookAtlasMeta>, anim_state: AnimState) -> FlipbookLanes {
    FlipbookLanes::fixed(frame_index_at(meta, anim_state, 0.0))
}

impl BubbleUniformGpu {
    /// Returns a copy of `snapshot` that keeps this uniform's optics lanes and CPU-only fields.
    ///
    /// Optics lanes belong to `bubble_optics`. `world_pos` is never read by `bubble_dome.wgsl`,
    /// because the mesh transform places the bubble, so a change in it alone would not alter one
    /// pixel. Every flipbook lane is read by the shader, so each is compared.
    fn normalized_to(&self, snapshot: &Self) -> Self {
        Self {
            world_pos: self.world_pos,
            optics_media: self.optics_media,
            optics_glare: self.optics_glare,
            optics_signals: self.optics_signals,
            optics_light: self.optics_light,
            ..*snapshot
        }
    }

    /// Whether applying `snapshot` would leave everything the fragment shader reads unchanged.
    #[must_use]
    pub fn renders_same_as(&self, snapshot: &Self) -> bool {
        *self == self.normalized_to(snapshot)
    }

    /// Writes the snapshot-owned fields of `snapshot`, preserving the optics lanes.
    pub fn apply_snapshot(&mut self, snapshot: &Self) {
        *self = Self {
            world_pos: snapshot.world_pos,
            ..self.normalized_to(snapshot)
        };
    }

    /// The flipbook lanes this uniform carries.
    #[must_use]
    pub const fn flipbook_lanes(&self) -> FlipbookLanes {
        FlipbookLanes {
            origin: self.anim_origin,
            base: self.frame_base,
            period: self.anim_period,
            count: self.frame_count,
            mode: self.anim_mode,
            offset: self.anim_offset,
        }
    }

    /// Returns this uniform with its flipbook lanes set to `lanes`.
    #[must_use]
    pub const fn with_flipbook_lanes(mut self, lanes: FlipbookLanes) -> Self {
        self.anim_origin = lanes.origin;
        self.frame_base = lanes.base;
        self.anim_period = lanes.period;
        self.frame_count = lanes.count;
        self.anim_mode = lanes.mode;
        self.anim_offset = lanes.offset;
        self
    }

    /// Packs a snapshot record with an already resolved linear ownership style and the
    /// flipbook lanes chosen for it.
    ///
    /// The record carries no animation phase; `lanes` say which atlas layers the clock plays.
    #[must_use]
    pub fn from_instance(
        instance: &BubbleRenderInstance,
        style: crate::player_presentation::PlayerStyle,
        lanes: FlipbookLanes,
    ) -> Self {
        let team_color = style.linear;

        let shape_kind = match instance.shape {
            ShapeKind::UnitSphere => 0,
            ShapeKind::BuildingDome { .. } => 1,
        };

        Self {
            team_color,
            world_pos: Vec3::new(
                instance.world_pos[0],
                instance.world_pos[2],
                instance.world_pos[1],
            ),
            radius: instance.radius,
            shape_kind,
            heading_angle_rad: instance.heading_angle_rad,
            aim_angle_rad: instance.aim_angle_rad,
            is_attacking: u32::from(instance.is_attacking),
            anim_state: instance.anim_state as u32,
            hp_ratio: instance.hp_ratio,
            shield_ratio: instance.shield_ratio,
            shield_hit_flash: instance.shield_hit_flash,
            hull_hit_angle_rad: instance.hull_hit_angle_rad,
            hull_hit_flash: instance.hull_hit_flash,
            build_progress: instance.build_or_queue_progress.unwrap_or(1.0),
            selected: u32::from(instance.selected),
            cloaked: u32::from(instance.concealment.is_concealed()),
            feature_flags: {
                let mut flags = DEFAULT_FEATURE_FLAGS;
                if instance.selected {
                    flags |= FLAG_SELECTED;
                }
                if instance.concealment.is_concealed() {
                    flags |= FLAG_CLOAKED;
                }
                if instance.speed_boosted {
                    flags |= FLAG_SPEED_BOOSTED;
                }
                flags
            },
            ..Self::default()
        }
        .with_flipbook_lanes(lanes)
    }
}

impl BubbleUniformGpu {
    /// Returns this uniform with its selection highlight set to `selected`.
    ///
    /// The simulation never reports a selection, so a live bubble's highlight is applied here.
    #[must_use]
    pub fn with_selection(mut self, selected: bool) -> Self {
        self.selected = u32::from(selected);
        if selected {
            self.feature_flags |= FLAG_SELECTED;
        } else {
            self.feature_flags &= !FLAG_SELECTED;
        }
        self
    }
}

/// Custom Bevy 3D material implementing the Bubble & Dome WGSL pipeline.
///
/// Build it with [`BubbleDomeMaterial::new`], which binds the shared flipbook clock.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct BubbleDomeMaterial {
    /// Primary per-bubble parameter uniform.
    #[uniform(0)]
    pub uniform: BubbleUniformGpu,
    /// `Texture2DArray` carrying all animation frames.
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    pub video_texture: Option<Handle<Image>>,
    /// The shared flipbook clock buffer, always [`FLIPBOOK_CLOCK_BUFFER`].
    #[storage(3, read_only, visibility(fragment))]
    pub flipbook_clock: Handle<ShaderStorageBuffer>,
    /// The effects' modulation of the bubble (sink, tint, rim, veil, soil); neutral unless the
    /// effect system holds a look on the unit ([`crate::effects::stance`]).
    #[uniform(4)]
    pub shell: ShellUniformGpu,
    /// Transparency blend mode.
    pub alpha_mode: AlphaMode,
}

impl BubbleDomeMaterial {
    /// A blended bubble material showing `video_texture` with `uniform`, bound to the shared
    /// flipbook clock, with a neutral shell.
    #[must_use]
    pub fn new(uniform: BubbleUniformGpu, video_texture: Option<Handle<Image>>) -> Self {
        Self {
            uniform,
            video_texture,
            flipbook_clock: FLIPBOOK_CLOCK_BUFFER,
            shell: ShellUniformGpu::default(),
            alpha_mode: AlphaMode::Blend,
        }
    }
}

/// Writes `shell` into the bubble material `handle` only when it differs, so a steady look
/// prepares nothing again (as [`sync_bubble_material`] does for the main uniform). Returns
/// whether the material was modified.
pub fn sync_bubble_shell(
    materials: &mut Assets<BubbleDomeMaterial>,
    handle: &Handle<BubbleDomeMaterial>,
    shell: &ShellUniformGpu,
) -> bool {
    if materials
        .get(handle)
        .is_none_or(|material| material.shell == *shell)
    {
        return false;
    }
    let Some(material) = materials.get_mut(handle) else {
        return false;
    };
    material.shell = *shell;
    true
}

/// Registers [`BubbleDomeMaterial`] together with the flipbook clock it binds.
///
/// Every app that draws bubbles adds this plugin instead of the bare `MaterialPlugin`, so the
/// clock buffer exists whenever a bubble material is prepared.
pub struct BubbleDomeMaterialPlugin;

impl Plugin for BubbleDomeMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<BubbleDomeMaterial>::default(),
            FlipbookClockPlugin,
        ));
    }
}

impl Material for BubbleDomeMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/bubble_dome.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/bubble_dome.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

/// Registry storing loaded flipbook manifests and GPU texture arrays.
#[derive(Resource, Default)]
pub struct FlipbookRegistry {
    /// Atlas metadata keyed by `UnitTypeId`.
    pub manifests_by_id: BTreeMap<UnitTypeId, FlipbookAtlasMeta>,
    /// GPU texture array handles keyed by `UnitTypeId`.
    pub textures_by_id: BTreeMap<UnitTypeId, Handle<Image>>,
    /// 2D strip images for UI portraits in egui keyed by `UnitTypeId`.
    pub portrait_images_by_id: BTreeMap<UnitTypeId, Handle<Image>>,
    /// Atlas metadata keyed by archetype name (e.g. for demo assets like "`concept_video`").
    pub manifests: BTreeMap<String, FlipbookAtlasMeta>,
    /// GPU texture array handles keyed by archetype name.
    pub textures: BTreeMap<String, Handle<Image>>,
    /// 2D strip images for UI portraits in egui.
    pub portrait_images: BTreeMap<String, Handle<Image>>,
}

impl FlipbookRegistry {
    /// Looks up metadata for the given unit type ID.
    #[must_use]
    pub fn get_meta(&self, unit_type: UnitTypeId) -> Option<&FlipbookAtlasMeta> {
        self.manifests_by_id.get(&unit_type)
    }

    /// Looks up only the exact admitted texture for the given unit type ID.
    #[must_use]
    pub fn get_texture(&self, unit_type: UnitTypeId) -> Option<Handle<Image>> {
        self.textures_by_id.get(&unit_type).cloned()
    }

    /// Looks up portrait image handle for the given unit type ID.
    #[must_use]
    pub fn get_portrait_by_id(&self, unit_type: UnitTypeId) -> Option<Handle<Image>> {
        self.portrait_images_by_id.get(&unit_type).cloned()
    }

    /// Looks up metadata by string name/slug (e.g. "`concept_video`").
    #[must_use]
    pub fn get_meta_by_name(&self, name: &str) -> Option<&FlipbookAtlasMeta> {
        self.manifests.get(name)
    }

    /// Looks up texture by string name/slug (e.g. "`concept_video`").
    #[must_use]
    pub fn get_texture_by_name(&self, name: &str) -> Option<Handle<Image>> {
        self.textures.get(name).cloned()
    }

    /// Looks up portrait image by string name/slug (e.g. "`concept_video`").
    #[must_use]
    pub fn get_portrait_by_name(&self, name: &str) -> Option<Handle<Image>> {
        self.portrait_images.get(name).cloned()
    }
}

/// Shared mesh handles for unit spheres and building domes.
#[derive(Resource)]
pub struct BubbleSharedMeshes {
    /// Unit mobile sphere mesh (radius 1.0).
    pub sphere_mesh: Handle<Mesh>,
    /// Building dome mesh (radius 1.0): a full sphere whose lower half lies below the ground.
    pub dome_mesh: Handle<Mesh>,
}

/// Sets up shared meshes and loads all texture array assets dynamically from `CatalogResource`.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn setup_bubble_assets_system(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut images: ResMut<'_, Assets<Image>>,
    catalog: Res<'_, crate::CatalogResource>,
    assets_root: Option<Res<'_, crate::ResolvedAssetsRoot>>,
    render_device: Option<Res<'_, bevy::render::renderer::RenderDevice>>,
    failure_policy: Option<Res<'_, crate::media::MediaFailurePolicy>>,
) {
    // 1. Mobile Unit Sphere Mesh (radius 1.0, high-tessellation UV sphere for smooth silhouette)
    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(80, 52));

    // 2. Building Dome Mesh (radius 1.0). A full sphere: its centre stands on the ground, which hides
    // the lower half.
    let dome = meshes.add(Sphere::new(1.0).mesh().uv(80, 52));

    commands.insert_resource(BubbleSharedMeshes {
        sphere_mesh: sphere,
        dome_mesh: dome,
    });

    let textures_dir = crate::assets::texture_assets_dir(assets_root.as_deref());
    let limits = render_device.as_ref().map(|device| {
        let limits = device.limits();
        asset_baker::media::TextureLimits {
            max_dimension_2d: limits.max_texture_dimension_2d,
            max_array_layers: limits.max_texture_array_layers,
        }
    });
    let (registry, status) = crate::media::load_registry(
        &textures_dir,
        &catalog.0,
        &mut images,
        limits,
        failure_policy.map_or_else(crate::media::MediaFailurePolicy::default, |policy| {
            policy.clone()
        }),
    );
    commands.insert_resource(status);
    commands.insert_resource(registry);
}

/// Canonical alias for the bubble synchronization system.
pub use crate::scene_controller::sync_live_battle_system as sync_bubbles_system;
