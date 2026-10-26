//! Diegetic tactical indicators: procedural SDF ground selection reticle and overhead HUD bars.
//!
//! # Architectural Responsibilities
//! This module renders tactical combat indicators:
//! - **Procedural SDF Holographic Selection Reticle:** Dual anti-aliased concentric rings, 4 rotating
//!   swept-arc corner brackets, 12 radial compass tick marks, and an inner ambient energy haze.
//! - **Floating Overhead Health & Shield Bar:** Billboarded dual-segment progress bar displaying live HP and shield ratios.
//! - **Air Unit Drop Shadow & Tether:** Ground shadow circle and vertical tether line for hovering air units.
//! - **Building Foundation Plate:** Metallic grid baseplate under building domes.
//!
//! Hull facing direction and attack target-lock aperture are rendered directly on the sphere's glass shell
//! in `bubble_dome.wgsl` (`FLAG_FACING_STRIPE` and `FLAG_ATTACK_APERTURE`).
//!
//! # Invariants
//! - `SelectionRingUniformGpu` memory layout is strictly 32 bytes and 16-byte aligned matching `selection_ring.wgsl`.
//! - Tactical reticle mesh is a flat ground quad plane pre-allocated once and shared across all entities.

#![allow(dead_code)]

use crate::bubble_shader::BubbleInstance;
use crate::lab_ui::LabState;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};
use sim_types::enums::{ShapeKind, SilhouetteClass};
use sim_types::snapshot::BubbleRenderInstance;

/// Marker for the ground selection ring entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundRingMarker;

/// Marker for legacy/optional equatorial hull facing chevron.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FacingChevronMarker;

/// Marker for legacy/optional dynamic attack focal lens aperture.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocalLensMarker;

/// Marker for the air unit ground shadow circle.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirShadowMarker;

/// Marker for the vertical air tether helper line.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirTetherMarker;

/// Marker for legacy/optional unit silhouette collar ornament.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollarMeshMarker {
    /// Active silhouette classification.
    pub silhouette: SilhouetteClass,
}

/// Marker for building foundation baseplates.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseplateMarker;

/// Marker for legacy rotating root entity holding corner brackets.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionBracketsRootMarker;

/// Marker for the floating overhead Health and Shield bar root.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadBarRootMarker;

/// Marker for the active Health fill mesh inside the overhead bar.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadHpBarMarker;

/// Marker for the active Shield fill mesh inside the overhead bar.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadShieldBarMarker;

/// GPU uniform buffer struct matching `struct SelectionRingUniform` in `selection_ring.wgsl`.
// PERFORMANCE RATIONALE:
// Why: WebGPU uniform buffers require strict 16-byte alignment and explicit std140 padding.
// What: Fields total exactly 32 bytes (16B Vec4 + 4x 4B fields).
// Trade-off: Explicit manual padding for zero runtime alignment faults.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
pub struct SelectionRingUniformGpu {
    /// Team tint color (RGBA).
    pub team_color: Vec4,
    /// Ground reticle radius in world units.
    pub radius: f32,
    /// Active selection visibility flag (1 = selected/visible, 0 = hidden).
    pub is_selected: u32,
    /// Breathing pulse oscillation speed.
    pub pulse_speed: f32,
    /// Explicit alignment padding to 32 bytes (16-byte aligned).
    pub padding: f32,
}

impl Default for SelectionRingUniformGpu {
    fn default() -> Self {
        Self {
            team_color: Vec4::new(0.2, 0.85, 1.0, 1.0),
            radius: 1.0,
            is_selected: 0,
            pulse_speed: 5.0,
            padding: 0.0,
        }
    }
}

/// Custom Bevy 3D material implementing the procedural SDF holographic selection reticle.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SelectionRingMaterial {
    /// GPU uniform buffer for selection ring reticle.
    #[uniform(0)]
    pub uniform: SelectionRingUniformGpu,
}

impl Material for SelectionRingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/selection_ring.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        0.0
    }
}

/// Shared mesh and material assets for tactical indicators.
#[derive(Resource)]
pub struct IndicatorAssets {
    /// Procedural SDF holographic ground selection plane mesh.
    pub selection_plane_mesh: Handle<Mesh>,
    /// Air drop shadow disc mesh.
    pub shadow_disc_mesh: Handle<Mesh>,
    /// Air vertical tether cylinder mesh.
    pub tether_line_mesh: Handle<Mesh>,
    /// Baseplate grid mesh for building domes.
    pub baseplate_mesh: Handle<Mesh>,
    // Overhead UI meshes
    /// Overhead bar background dark border quad.
    pub bar_bg_mesh: Handle<Mesh>,
    /// Overhead HP fill quad.
    pub hp_bar_mesh: Handle<Mesh>,
    /// Overhead Shield fill quad.
    pub shield_bar_mesh: Handle<Mesh>,
    // Indicator materials
    /// High-health green material.
    pub hp_green_mat: Handle<StandardMaterial>,
    /// Mid-health gold/yellow material.
    pub hp_yellow_mat: Handle<StandardMaterial>,
    /// Critical health red material.
    pub hp_red_mat: Handle<StandardMaterial>,
    /// Shield cyan emissive material.
    pub shield_cyan_mat: Handle<StandardMaterial>,
    /// Dark HUD bar background material.
    pub bar_bg_mat: Handle<StandardMaterial>,
}

// PERFORMANCE RATIONALE:
// Why: Generating procedural indicator meshes on every frame causes catastrophic memory allocation.
// What: All indicator meshes (selection ground quad, shadows, bars) are pre-allocated once
// at startup and shared across all active bubble instances.
// Trade-off: Fixed set of indicator geometries in exchange for zero garbage collection during battle.
/// Initializes pre-allocated indicator meshes and shared materials.
pub fn setup_indicator_assets_system(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
) {
    let hp_green_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.9, 0.3),
        emissive: LinearRgba::rgb(0.3, 1.8, 0.4),
        unlit: true,
        ..default()
    });

    let hp_yellow_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.85, 0.2),
        emissive: LinearRgba::rgb(2.0, 1.6, 0.3),
        unlit: true,
        ..default()
    });

    let hp_red_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.2, 0.15),
        emissive: LinearRgba::rgb(2.5, 0.4, 0.3),
        unlit: true,
        ..default()
    });

    let shield_cyan_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.8, 1.0),
        emissive: LinearRgba::rgb(0.4, 2.0, 3.2),
        unlit: true,
        ..default()
    });

    let bar_bg_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.04, 0.05, 0.08),
        unlit: true,
        ..default()
    });

    let assets = IndicatorAssets {
        selection_plane_mesh: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
        shadow_disc_mesh: meshes.add(Cylinder::new(0.9, 0.02)),
        tether_line_mesh: meshes.add(Cylinder::new(0.02, 1.0)),
        baseplate_mesh: meshes.add(Cuboid::new(2.4, 0.1, 2.4)),
        bar_bg_mesh: meshes.add(Cuboid::new(1.04, 0.14, 0.02)),
        hp_bar_mesh: meshes.add(Cuboid::new(1.0, 0.07, 0.03)),
        shield_bar_mesh: meshes.add(Cuboid::new(1.0, 0.04, 0.03)),
        hp_green_mat,
        hp_yellow_mat,
        hp_red_mat,
        shield_cyan_mat,
        bar_bg_mat,
    };
    commands.insert_resource(assets);
}

/// Spawns indicator child entities (SDF selection reticle plane, overhead HP/Shield bar) under a bubble entity.
pub fn attach_bubble_indicators(
    commands: &mut Commands<'_, '_>,
    parent: Entity,
    instance: &BubbleRenderInstance,
    assets: &IndicatorAssets,
    materials: &mut Assets<StandardMaterial>,
    ring_materials: &mut Assets<SelectionRingMaterial>,
) {
    let team_color = match instance.team_id.0 {
        1 => Color::srgb(0.2, 0.6, 1.0),
        2 => Color::srgb(1.0, 0.25, 0.2),
        _ => Color::srgb(0.8, 0.8, 0.8),
    };

    let team_vec4 = match instance.team_id.0 {
        1 => Vec4::new(0.2, 0.6, 1.0, 1.0),
        2 => Vec4::new(1.0, 0.25, 0.2, 1.0),
        _ => Vec4::new(0.8, 0.8, 0.8, 1.0),
    };

    let team_mat = materials.add(StandardMaterial {
        base_color: team_color,
        emissive: LinearRgba::from(team_color) * 1.5,
        unlit: true,
        ..default()
    });

    let shadow_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.6),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let is_dome = matches!(instance.shape, ShapeKind::BuildingDome { .. });
    let ground_y_offset = if is_dome { 0.02 } else { -0.98 };
    let initial_selection_vis = if instance.selected {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    let ring_mat_handle = ring_materials.add(SelectionRingMaterial {
        uniform: SelectionRingUniformGpu {
            team_color: team_vec4,
            radius: instance.radius,
            is_selected: u32::from(instance.selected),
            pulse_speed: 5.0,
            padding: 0.0,
        },
    });

    commands.entity(parent).with_children(|builder| {
        // 1. Procedural SDF Holographic Ground Selection Reticle Quad Plane
        builder.spawn((
            Mesh3d(assets.selection_plane_mesh.clone()),
            MeshMaterial3d(ring_mat_handle),
            Transform::from_xyz(0.0, ground_y_offset, 0.0).with_scale(Vec3::splat(2.8)),
            initial_selection_vis,
            GroundRingMarker,
        ));

        // 2. Floating Overhead Health & Shield Bar
        builder
            .spawn((
                Transform::from_xyz(0.0, 1.35, 0.0)
                    .with_scale(Vec3::splat(1.0 / instance.radius.max(0.4))),
                initial_selection_vis,
                OverheadBarRootMarker,
            ))
            .with_children(|bar_builder| {
                // Background dark border
                bar_builder.spawn((
                    Mesh3d(assets.bar_bg_mesh.clone()),
                    MeshMaterial3d(assets.bar_bg_mat.clone()),
                    Transform::IDENTITY,
                ));
                // Green HP bar (upper half)
                bar_builder.spawn((
                    Mesh3d(assets.hp_bar_mesh.clone()),
                    MeshMaterial3d(assets.hp_green_mat.clone()),
                    Transform::from_xyz(0.0, 0.025, 0.01),
                    OverheadHpBarMarker,
                ));
                // Cyan Shield bar (lower half)
                bar_builder.spawn((
                    Mesh3d(assets.shield_bar_mesh.clone()),
                    MeshMaterial3d(assets.shield_cyan_mat.clone()),
                    Transform::from_xyz(0.0, -0.035, 0.01),
                    OverheadShieldBarMarker,
                ));
            });

        // 3. Air Drop Shadow & Tether Line (for flying bubbles)
        if instance.world_pos[2] > instance.ground_z + 0.5 {
            let height = instance.world_pos[2] - instance.ground_z;
            let inv_r = 1.0 / instance.radius.max(0.4);
            builder.spawn((
                Mesh3d(assets.shadow_disc_mesh.clone()),
                MeshMaterial3d(shadow_mat),
                Transform::from_xyz(0.0, -height * inv_r, 0.0)
                    .with_scale(Vec3::new(inv_r, 1.0, inv_r)),
                AirShadowMarker,
            ));

            builder.spawn((
                Mesh3d(assets.tether_line_mesh.clone()),
                MeshMaterial3d(team_mat.clone()),
                Transform::from_xyz(0.0, -height * 0.5 * inv_r, 0.0).with_scale(Vec3::new(
                    inv_r,
                    height * inv_r,
                    inv_r,
                )),
                AirTetherMarker,
            ));
        }
    });
}

/// Updates visibility, uniform parameters, and scales of selection reticle and overhead bars.
#[allow(clippy::type_complexity)]
pub fn update_indicators_system(
    lab_state: Option<Res<'_, LabState>>,
    assets: Res<'_, IndicatorAssets>,
    instance_query: Query<'_, '_, (&BubbleInstance, &Children)>,
    children_query: Query<'_, '_, &Children>,
    mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>,
    mut ring_query: Query<
        '_,
        '_,
        (&MeshMaterial3d<SelectionRingMaterial>, &mut Visibility),
        With<GroundRingMarker>,
    >,
    mut overhead_query: Query<
        '_,
        '_,
        &mut Visibility,
        (With<OverheadBarRootMarker>, Without<GroundRingMarker>),
    >,
    mut hp_bar_query: Query<
        '_,
        '_,
        (&mut Transform, &mut MeshMaterial3d<StandardMaterial>),
        (
            With<OverheadHpBarMarker>,
            Without<GroundRingMarker>,
            Without<OverheadBarRootMarker>,
        ),
    >,
    mut shield_bar_query: Query<
        '_,
        '_,
        &mut Transform,
        (
            With<OverheadShieldBarMarker>,
            Without<GroundRingMarker>,
            Without<OverheadBarRootMarker>,
            Without<OverheadHpBarMarker>,
        ),
    >,
) {
    let opt_brackets = lab_state
        .as_ref()
        .map_or(true, |s| s.opt_selection_brackets);

    for (instance_wrapper, children) in instance_query.iter() {
        let instance = &instance_wrapper.0;
        let is_selected = instance.selected && opt_brackets;
        let selection_vis = if is_selected {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        for &child in children {
            // 1. Update Holographic Ground Reticle Uniform & Visibility
            if let Ok((ring_mat_handle, mut ring_vis)) = ring_query.get_mut(child) {
                *ring_vis = selection_vis;
                if let Some(mat) = ring_materials.get_mut(&ring_mat_handle.0) {
                    mat.uniform.is_selected = u32::from(is_selected);
                }
            }

            // 2. Update Floating Overhead Health & Shield Bar Root and its Children
            if let Ok(mut bar_vis) = overhead_query.get_mut(child) {
                *bar_vis = selection_vis;

                if let Ok(bar_children) = children_query.get(child) {
                    for &grandchild in bar_children {
                        // Update Health Bar Scale & Dynamic Material Threshold
                        if let Ok((mut hp_tf, mut hp_mat)) = hp_bar_query.get_mut(grandchild) {
                            let clamped_hp = instance.hp_ratio.clamp(0.0, 1.0);
                            hp_tf.scale.x = clamped_hp;
                            hp_tf.translation.x = (clamped_hp - 1.0) * 0.5;

                            let target_mat = if clamped_hp > 0.50 {
                                assets.hp_green_mat.clone()
                            } else if clamped_hp > 0.25 {
                                assets.hp_yellow_mat.clone()
                            } else {
                                assets.hp_red_mat.clone()
                            };
                            *hp_mat = MeshMaterial3d(target_mat);
                        }

                        // Update Shield Bar Scale
                        if let Ok(mut shield_tf) = shield_bar_query.get_mut(grandchild) {
                            let clamped_shield = instance.shield_ratio.clamp(0.0, 1.0);
                            shield_tf.scale.x = clamped_shield;
                            shield_tf.translation.x = (clamped_shield - 1.0) * 0.5;
                        }
                    }
                }
            }
        }
    }
}
