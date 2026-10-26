//! Custom Bevy Material and GPU uniform binding for Unit Bubbles and Building Domes.
//!
//! # Architectural Responsibilities
//! This module binds CPU simulation snapshots into GPU-visible `BubbleUniformGpu` structures
//! and manages the `BubbleDomeMaterial` shader pipeline. It translates vertical strip flipbooks
//! into high-performance `Texture2DArray` assets and samples active frames with zero heap allocations.
//!
//! # Invariants
//! - `BubbleUniformGpu` memory layout is strictly 96 bytes and 16-byte aligned matching `bubble_dome.wgsl`.
//! - Mobile units render as full UV spheres; building structures render as grounded hemisphere domes.
//! - When texture assets are unavailable (e.g. headless CI), a fallback 1-layer texture array is used.

#![allow(dead_code)]

use asset_baker::manifest::FlipbookAtlasMeta;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDimension, TextureFormat,
    TextureViewDescriptor, TextureViewDimension,
};
use sim_types::enums::{AnimState, ShapeKind};
use sim_types::ids::UnitTypeId;
use sim_types::snapshot::BubbleRenderInstance;
use std::collections::BTreeMap;

/// ECS Component wrapping authoritative [`BubbleRenderInstance`] snapshot data.
#[derive(Component, Clone, Debug, Deref, DerefMut)]
pub struct BubbleInstance(pub BubbleRenderInstance);

/// GPU uniform buffer struct matching `struct BubbleUniform` in `bubble_dome.wgsl`.
// PERFORMANCE RATIONALE:
// Why: WebGPU uniform buffers require strict 16-byte alignment and explicit std140 padding.
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
/// Feature bitflag: Speed boost / stimpack hyper wake.
pub const FLAG_SPEED_BOOSTED: u32 = 1 << 7;
/// Feature bitflag: Direct-on-sphere holographic facing stripe & nose chevron.
pub const FLAG_FACING_STRIPE: u32 = 1 << 8;
/// Feature bitflag: Attack target-lock aperture ring on sphere.
pub const FLAG_ATTACK_APERTURE: u32 = 1 << 9;

/// Default active optical features.
pub const DEFAULT_FEATURE_FLAGS: u32 = FLAG_GLARE
    | FLAG_FRESNEL
    | FLAG_INNER_WARP
    | FLAG_CRACKS
    | FLAG_CRT_GLITCH
    | FLAG_FACING_STRIPE
    | FLAG_ATTACK_APERTURE;

/// GPU uniform buffer struct matching `struct BubbleUniform` in `bubble_dome.wgsl`.
// PERFORMANCE RATIONALE:
// Why: WebGPU uniform buffers require strict 16-byte alignment and explicit std140 padding.
// What: Fields are strictly ordered: vec4 (16B), vec3+f32 (16B), 4x u32/f32 (16B) chunks, totaling exactly 96 bytes.
// Modular optical feature toggles are packed into `feature_flags: u32` bitmask preserving the exact 96-byte memory footprint.
// Trade-off: Manual struct layout in exchange for zero runtime padding overhead and zero shader misalignment faults.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
#[allow(dead_code)]
pub struct BubbleUniformGpu {
    /// Team Fresnel rim and ground indicator color (RGBA).
    pub team_color: Vec4,
    /// Authoritative 3D center position [x, ground_y, altitude_z].
    pub world_pos: Vec3,
    /// Sphere collision radius in world units.
    pub radius: f32,
    /// 0 = UnitSphere (mobile), 1 = BuildingDome (grounded structure).
    pub shape_kind: u32,
    /// Facing orientation angle in radians.
    pub heading_angle_rad: f32,
    /// Independent weapon turret aim angle in radians.
    pub aim_angle_rad: f32,
    /// Active firing / windup state flag (1 = firing, 0 = idle).
    pub is_attacking: u32,
    /// High-level animation action state index.
    pub anim_state: u32,
    /// Normalized animation progress [0.0, 1.0].
    pub anim_progress: f32,
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
    /// Vertical construction wireframe scanline fill [0.0, 1.0].
    pub build_progress: f32,
    /// Selection highlight flag (1 = selected, 0 = unselected).
    pub selected: u32,
    /// Stealth cloaking flag (1 = cloaked shimmer, 0 = opaque).
    pub cloaked: u32,
    /// Modular feature bitflags bitmask (glare, fresnel, inner warp, cracks, scanlines).
    pub feature_flags: u32,
    /// Exact active frame index in the Texture2DArray.
    pub frame_index: u32,
}

impl Default for BubbleUniformGpu {
    fn default() -> Self {
        Self {
            team_color: Vec4::new(0.2, 0.6, 1.0, 1.0),
            world_pos: Vec3::ZERO,
            radius: 0.5,
            shape_kind: 0,
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: 0,
            anim_state: 1, // Idle
            anim_progress: 0.0,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_progress: 1.0,
            selected: 0,
            cloaked: 0,
            feature_flags: DEFAULT_FEATURE_FLAGS,
            frame_index: 0,
        }
    }
}

impl BubbleUniformGpu {
    /// Translates a simulation [`BubbleRenderInstance`] and optional metadata into a GPU uniform.
    #[must_use]
    pub fn from_instance(
        instance: &BubbleRenderInstance,
        meta: Option<&FlipbookAtlasMeta>,
    ) -> Self {
        let team_color = match instance.team_id.0 {
            1 => Vec4::new(0.18, 0.55, 1.0, 1.0), // Blue Team
            2 => Vec4::new(1.0, 0.22, 0.18, 1.0), // Red Team
            _ => Vec4::new(0.7, 0.7, 0.7, 1.0),
        };

        let shape_kind = match instance.shape {
            ShapeKind::UnitSphere => 0,
            ShapeKind::BuildingDome { .. } => 1,
        };

        let frame_index = if let Some(meta) = meta {
            if let Some(slice) = meta.actions.get(&instance.anim_state) {
                if slice.frame_count == 0 {
                    slice.start_frame
                } else if slice.looping {
                    let step = (instance.anim_progress * slice.frame_count as f32) as u32;
                    slice.start_frame + (step % slice.frame_count)
                } else {
                    let step = (instance.anim_progress * slice.frame_count as f32) as u32;
                    slice.start_frame + step.min(slice.frame_count.saturating_sub(1))
                }
            } else {
                0
            }
        } else {
            // Default 8-frame slicing per AnimState
            let state_offset = match instance.anim_state {
                AnimState::Constructing => 0,
                AnimState::Idle => 8,
                AnimState::Walk => 16,
                AnimState::Attack | AnimState::Cast => 24,
                AnimState::Die => 32,
            };
            state_offset + ((instance.anim_progress * 8.0) as u32 % 8)
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
            anim_progress: instance.anim_progress,
            hp_ratio: instance.hp_ratio,
            shield_ratio: instance.shield_ratio,
            shield_hit_flash: instance.shield_hit_flash,
            hull_hit_angle_rad: instance.hull_hit_angle_rad,
            hull_hit_flash: instance.hull_hit_flash,
            build_progress: instance.build_or_queue_progress.unwrap_or(1.0),
            selected: u32::from(instance.selected),
            cloaked: u32::from(instance.cloaked),
            feature_flags: {
                let mut flags = DEFAULT_FEATURE_FLAGS;
                if instance.selected {
                    flags |= FLAG_SELECTED;
                }
                if instance.cloaked {
                    flags |= FLAG_CLOAKED;
                }
                if instance.speed_boosted {
                    flags |= FLAG_SPEED_BOOSTED;
                }
                flags
            },
            frame_index,
        }
    }
}

/// Custom Bevy 3D material implementing the Bubble & Dome WGSL pipeline.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct BubbleDomeMaterial {
    /// Primary per-bubble parameter uniform.
    #[uniform(0)]
    pub uniform: BubbleUniformGpu,
    /// Texture2DArray carrying all animation frames.
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    pub video_texture: Option<Handle<Image>>,
    /// Transparency blend mode.
    pub alpha_mode: AlphaMode,
}

impl Material for BubbleDomeMaterial {
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
    /// Atlas metadata keyed by UnitTypeId.
    pub manifests_by_id: BTreeMap<UnitTypeId, FlipbookAtlasMeta>,
    /// GPU texture array handles keyed by UnitTypeId.
    pub textures_by_id: BTreeMap<UnitTypeId, Handle<Image>>,
    /// 2D strip images for UI portraits in egui keyed by UnitTypeId.
    pub portrait_images_by_id: BTreeMap<UnitTypeId, Handle<Image>>,
    /// Fallback 1-layer texture array for missing assets.
    pub fallback_texture: Option<Handle<Image>>,
    /// Atlas metadata keyed by archetype name (e.g. for demo assets like "concept_video").
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

    /// Looks up the texture handle for the given unit type ID, or falls back.
    #[must_use]
    pub fn get_texture(&self, unit_type: UnitTypeId) -> Option<Handle<Image>> {
        self.textures_by_id
            .get(&unit_type)
            .cloned()
            .or_else(|| self.fallback_texture.clone())
    }

    /// Looks up portrait image handle for the given unit type ID.
    #[must_use]
    pub fn get_portrait_by_id(&self, unit_type: UnitTypeId) -> Option<Handle<Image>> {
        self.portrait_images_by_id.get(&unit_type).cloned()
    }

    /// Looks up metadata by string name/slug (e.g. "concept_video").
    #[must_use]
    pub fn get_meta_by_name(&self, name: &str) -> Option<&FlipbookAtlasMeta> {
        self.manifests.get(name)
    }

    /// Looks up texture by string name/slug (e.g. "concept_video").
    #[must_use]
    pub fn get_texture_by_name(&self, name: &str) -> Option<Handle<Image>> {
        self.textures
            .get(name)
            .cloned()
            .or_else(|| self.fallback_texture.clone())
    }

    /// Looks up portrait image by string name/slug (e.g. "concept_video").
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
    /// Building dome hemisphere mesh (radius 1.0, y >= 0.0).
    pub dome_mesh: Handle<Mesh>,
}

/// Sets up shared meshes and loads all texture array assets dynamically from `CatalogResource`.
pub fn setup_bubble_assets_system(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut images: ResMut<'_, Assets<Image>>,
    catalog: Res<'_, crate::CatalogResource>,
) {
    // 1. Mobile Unit Sphere Mesh (radius 1.0, high-tessellation UV sphere for smooth silhouette)
    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(80, 52));

    // 2. Building Dome Hemisphere Mesh (radius 1.0, high-tessellation UV hemisphere)
    let dome = meshes.add(Sphere::new(1.0).mesh().uv(80, 52));

    commands.insert_resource(BubbleSharedMeshes {
        sphere_mesh: sphere,
        dome_mesh: dome,
    });

    // 3. Create Fallback 1x1 1-layer Texture2DArray (pure neutral blue-gray)
    let fallback_data = vec![25, 30, 42, 255];
    let mut fallback_img = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        fallback_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    fallback_img.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    let fallback_handle = images.add(fallback_img);

    let mut registry = FlipbookRegistry {
        manifests_by_id: BTreeMap::new(),
        textures_by_id: BTreeMap::new(),
        portrait_images_by_id: BTreeMap::new(),
        manifests: BTreeMap::new(),
        textures: BTreeMap::new(),
        portrait_images: BTreeMap::new(),
        fallback_texture: Some(fallback_handle),
    };

    // 4. Load all available flipbooks dynamically from workspace assets/textures/
    let textures_dir = crate::resolve_assets_dir().join("textures");

    for arch in catalog.0.archetypes() {
        let name = &arch.slug;
        let unit_type = arch.unit_type_id;
        let ron_path = textures_dir.join(format!("{name}.flipbook.ron"));
        let png_path = textures_dir.join(format!("{name}.png"));

        if ron_path.exists() && png_path.exists() {
            if let Ok(meta) = FlipbookAtlasMeta::load_from_ron(&ron_path) {
                if let Ok(img) = image::open(&png_path) {
                    let rgba = img.to_rgba8();
                    let raw_bytes = rgba.into_raw();
                    let expected_bytes =
                        (meta.frame_width * meta.frame_height * 4 * meta.total_frames) as usize;

                    if raw_bytes.len() == expected_bytes {
                        let mut array_img = Image::new(
                            Extent3d {
                                width: meta.frame_width,
                                height: meta.frame_height,
                                depth_or_array_layers: meta.total_frames,
                            },
                            TextureDimension::D2,
                            raw_bytes.clone(),
                            TextureFormat::Rgba8UnormSrgb,
                            RenderAssetUsages::default(),
                        );
                        array_img.texture_view_descriptor = Some(TextureViewDescriptor {
                            dimension: Some(TextureViewDimension::D2Array),
                            ..default()
                        });
                        let handle = images.add(array_img);
                        registry.textures_by_id.insert(unit_type, handle.clone());
                        registry.textures.insert(name.clone(), handle);

                        let portrait_img = Image::new(
                            Extent3d {
                                width: meta.frame_width,
                                height: meta.frame_height * meta.total_frames,
                                depth_or_array_layers: 1,
                            },
                            TextureDimension::D2,
                            raw_bytes,
                            TextureFormat::Rgba8UnormSrgb,
                            RenderAssetUsages::default(),
                        );
                        let portrait_handle = images.add(portrait_img);
                        registry
                            .portrait_images_by_id
                            .insert(unit_type, portrait_handle.clone());
                        registry
                            .portrait_images
                            .insert(name.clone(), portrait_handle);
                    }
                }
                registry.manifests_by_id.insert(unit_type, meta.clone());
                registry.manifests.insert(name.clone(), meta);
            }
        }
    }

    // Also load "concept_video" if present for Visual Lab demo
    let concept_name = "concept_video";
    let concept_ron = textures_dir.join(format!("{concept_name}.flipbook.ron"));
    let concept_png = textures_dir.join(format!("{concept_name}.png"));
    if concept_ron.exists() && concept_png.exists() {
        if let Ok(meta) = FlipbookAtlasMeta::load_from_ron(&concept_ron) {
            if let Ok(img) = image::open(&concept_png) {
                let rgba = img.to_rgba8();
                let raw_bytes = rgba.into_raw();
                let expected_bytes =
                    (meta.frame_width * meta.frame_height * 4 * meta.total_frames) as usize;
                if raw_bytes.len() == expected_bytes {
                    let mut array_img = Image::new(
                        Extent3d {
                            width: meta.frame_width,
                            height: meta.frame_height,
                            depth_or_array_layers: meta.total_frames,
                        },
                        TextureDimension::D2,
                        raw_bytes.clone(),
                        TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::default(),
                    );
                    array_img.texture_view_descriptor = Some(TextureViewDescriptor {
                        dimension: Some(TextureViewDimension::D2Array),
                        ..default()
                    });
                    let handle = images.add(array_img);
                    registry.textures.insert(concept_name.to_string(), handle);

                    let portrait_img = Image::new(
                        Extent3d {
                            width: meta.frame_width,
                            height: meta.frame_height * meta.total_frames,
                            depth_or_array_layers: 1,
                        },
                        TextureDimension::D2,
                        raw_bytes,
                        TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::default(),
                    );
                    let portrait_handle = images.add(portrait_img);
                    registry
                        .portrait_images
                        .insert(concept_name.to_string(), portrait_handle);
                }
            }
            registry.manifests.insert(concept_name.to_string(), meta);
        }
    }

    // 5. Dynamic placeholder binding for archetypes lacking on-disk textures:
    // Discover fallback handles by matching loaded archetypes by faction and shape kind.
    // Zero hardcoded creature / building slugs in Rust (Charter 3.3).
    for arch in catalog.0.archetypes() {
        if registry.textures_by_id.contains_key(&arch.unit_type_id) {
            continue;
        }

        let is_dome = matches!(arch.shape, ShapeKind::BuildingDome { .. });

        let candidate_id = catalog
            .0
            .archetypes()
            .find(|cand| {
                registry.textures_by_id.contains_key(&cand.unit_type_id)
                    && matches!(cand.shape, ShapeKind::BuildingDome { .. }) == is_dome
                    && cand.faction == arch.faction
            })
            .or_else(|| {
                catalog.0.archetypes().find(|cand| {
                    registry.textures_by_id.contains_key(&cand.unit_type_id)
                        && matches!(cand.shape, ShapeKind::BuildingDome { .. }) == is_dome
                })
            })
            .or_else(|| {
                catalog
                    .0
                    .archetypes()
                    .find(|cand| registry.textures_by_id.contains_key(&cand.unit_type_id))
            })
            .map(|cand| cand.unit_type_id);

        if let Some(src_id) = candidate_id {
            if let Some(tex) = registry.textures_by_id.get(&src_id).cloned() {
                registry
                    .textures_by_id
                    .insert(arch.unit_type_id, tex.clone());
                registry.textures.insert(arch.slug.clone(), tex);
            }
            if let Some(meta) = registry.manifests_by_id.get(&src_id).cloned() {
                registry
                    .manifests_by_id
                    .insert(arch.unit_type_id, meta.clone());
                registry.manifests.insert(arch.slug.clone(), meta);
            }
            if let Some(portrait) = registry.portrait_images_by_id.get(&src_id).cloned() {
                registry
                    .portrait_images_by_id
                    .insert(arch.unit_type_id, portrait.clone());
                registry.portrait_images.insert(arch.slug.clone(), portrait);
            }
        }
    }

    commands.insert_resource(registry);
}

/// Canonical alias for the bubble synchronization system.
pub use crate::scene_controller::sync_live_battle_system as sync_bubbles_system;
