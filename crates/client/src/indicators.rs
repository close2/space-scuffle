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
//! - Every bubble has the same children: ring, bar root (background, HP and shield bars), air
//!   shadow and tether; the shadow and tether are hidden on a bubble that is not airborne when it
//!   arrives. [`IndicatorPose::of`] is the only source of their values, for a fresh spawn
//!   ([`attach_bubble_indicators`]) and for a reused render entity ([`IndicatorWriter`]), so a
//!   reused bubble's children are a fresh bubble's (plan `2026-11-29-render-entity-pool` D-5, D-6).
//! - A parked render entity ([`ParkedBattleBubble`]) is never written by
//!   [`update_indicators_system`].

use crate::bubble_shader::BubbleInstance;
use crate::lab_ui::LabState;
use crate::render_pool::ParkedBattleBubble;
use crate::scene_controller::LiveBattleBubble;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use sim_types::enums::ShapeKind;
use sim_types::snapshot::BubbleRenderInstance;

/// Marker for the ground selection ring entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroundRingMarker;

/// Marker for the air unit ground shadow circle.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirShadowMarker;

/// Marker for the vertical air tether helper line.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirTetherMarker;

/// Marker for the floating overhead Health and Shield bar root.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadBarRootMarker;

/// Marker for the active Health fill mesh inside the overhead bar.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadHpBarMarker;

/// Marker for the active Shield fill mesh inside the overhead bar.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverheadShieldBarMarker;

mod gpu_layout {
    //! Holds one `ShaderType` struct, so the lint expectation below covers nothing else.
    #![expect(
        dead_code,
        reason = "the ShaderType derive emits layout checks nothing calls"
    )]

    use bevy::prelude::*;
    use bevy::render::render_resource::ShaderType;

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
}
pub use gpu_layout::SelectionRingUniformGpu;

impl Default for SelectionRingUniformGpu {
    fn default() -> Self {
        Self {
            team_color: Vec4::ZERO,
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

/// Height above its ground, world units, beyond which a bubble is airborne and shows its air
/// shadow and tether.
const AIRBORNE_HEIGHT: f32 = 0.5;

/// Height of the air shadow disc's centre above its record's ground, world units (plan
/// `2026-11-30-terrain-and-air-indicators` D-5). The disc is `0.02` radii thick and centred, so its
/// underside stays above the ground for every drawn radius below `2`.
pub const SHADOW_LIFT: f32 = 0.02;

/// Scale of the selection reticle plane relative to its bubble.
const RING_SCALE: f32 = 2.8;

/// Height of the overhead bar root above the bubble's centre, in bubble radii.
const BAR_HEIGHT: f32 = 1.35;

/// Smallest radius the overhead bar and the air indicators are scaled against, world units.
const MIN_INDICATOR_RADIUS: f32 = 0.4;

/// The HP bar at rest: full, centred in the bar, in front of the background.
const HP_BAR_REST: Transform = Transform::from_xyz(0.0, 0.025, 0.01);

/// The shield bar at rest: full, below the HP bar.
const SHIELD_BAR_REST: Transform = Transform::from_xyz(0.0, -0.035, 0.01);

/// Breathing pulse of the selection reticle.
const RING_PULSE_SPEED: f32 = 5.0;

/// Where and how every indicator child of one bubble is drawn when the bubble arrives: the one
/// source of child values for a fresh spawn and for a reused render entity (plan
/// `2026-11-29-render-entity-pool` D-5, D-6). Transforms are relative to the bubble, whose scale
/// is its radius in world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndicatorPose {
    /// The selection reticle plane.
    pub ring: Transform,
    /// The reticle's material uniform: team colour, radius and selection.
    pub ring_uniform: SelectionRingUniformGpu,
    /// Visibility of the reticle and the overhead bar: shown while selected.
    pub selection: Visibility,
    /// The overhead bar root.
    pub bar_root: Transform,
    /// The HP bar, full.
    pub hp_bar: Transform,
    /// The shield bar, full.
    pub shield_bar: Transform,
    /// The air shadow disc.
    pub shadow: Transform,
    /// The air tether line.
    pub tether: Transform,
    /// Visibility of the air shadow and tether: shown when airborne at arrival.
    pub air: Visibility,
    /// The owner's colour, of the tether's team material.
    pub team_color: Color,
}

impl IndicatorPose {
    /// The pose of `instance`'s indicators in its owner's `style`. Pure.
    #[must_use]
    pub fn of(
        instance: &BubbleRenderInstance,
        style: crate::player_presentation::PlayerStyle,
    ) -> Self {
        let is_dome = matches!(instance.shape, ShapeKind::BuildingDome { .. });
        let ground_y_offset = if is_dome { 0.02 } else { -0.98 };
        let selection = if instance.selected {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let height = instance.world_pos[2] - instance.ground_z;
        let airborne = height > AIRBORNE_HEIGHT;
        // A bubble on the ground keeps hidden air indicators of the least airborne height, so
        // its transforms never collapse to a zero scale.
        let height = height.max(AIRBORNE_HEIGHT);
        let inv_r = 1.0 / instance.radius.max(MIN_INDICATOR_RADIUS);
        Self {
            ring: Transform::from_xyz(0.0, ground_y_offset, 0.0)
                .with_scale(Vec3::splat(RING_SCALE)),
            ring_uniform: SelectionRingUniformGpu {
                team_color: style.linear,
                radius: instance.radius,
                is_selected: u32::from(instance.selected),
                pulse_speed: RING_PULSE_SPEED,
                padding: 0.0,
            },
            selection,
            bar_root: Transform::from_xyz(0.0, BAR_HEIGHT, 0.0).with_scale(Vec3::splat(inv_r)),
            hp_bar: HP_BAR_REST,
            shield_bar: SHIELD_BAR_REST,
            shadow: Transform::from_xyz(0.0, -height * inv_r, 0.0)
                .with_scale(Vec3::new(inv_r, 1.0, inv_r)),
            tether: Transform::from_xyz(0.0, -height * 0.5 * inv_r, 0.0).with_scale(Vec3::new(
                inv_r,
                height * inv_r,
                inv_r,
            )),
            air: if airborne {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            team_color: style.color(),
        }
    }
}

/// The team material of a bubble's tether: unlit, glowing in the owner's colour.
fn team_material(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        emissive: LinearRgba::from(color) * 1.5,
        unlit: true,
        ..default()
    }
}

/// Spawns indicator child entities (SDF selection reticle plane, overhead HP/Shield bar, air
/// shadow and tether) under a bubble entity, placed by [`IndicatorPose::of`].
// PERFORMANCE RATIONALE:
// Why: A render pool can reuse any parked bubble only if all bubbles have the same children.
// What: Every bubble carries the air shadow and tether, hidden on the ground.
// Trade-off: Two hidden entities and two retained materials per ground bubble.
pub fn attach_bubble_indicators(
    commands: &mut Commands<'_, '_>,
    parent: Entity,
    instance: &BubbleRenderInstance,
    assets: &IndicatorAssets,
    materials: &mut Assets<StandardMaterial>,
    ring_materials: &mut Assets<SelectionRingMaterial>,
    style: crate::player_presentation::PlayerStyle,
) {
    let pose = IndicatorPose::of(instance, style);
    let team_mat = materials.add(team_material(pose.team_color));
    let shadow_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.6),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let ring_mat_handle = ring_materials.add(SelectionRingMaterial {
        uniform: pose.ring_uniform,
    });

    commands.entity(parent).with_children(|builder| {
        // 1. Procedural SDF Holographic Ground Selection Reticle Quad Plane
        builder.spawn((
            Mesh3d(assets.selection_plane_mesh.clone()),
            MeshMaterial3d(ring_mat_handle),
            pose.ring,
            pose.selection,
            GroundRingMarker,
        ));

        // 2. Floating Overhead Health & Shield Bar
        builder
            .spawn((pose.bar_root, pose.selection, OverheadBarRootMarker))
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
                    pose.hp_bar,
                    OverheadHpBarMarker,
                ));
                // Cyan Shield bar (lower half)
                bar_builder.spawn((
                    Mesh3d(assets.shield_bar_mesh.clone()),
                    MeshMaterial3d(assets.shield_cyan_mat.clone()),
                    pose.shield_bar,
                    OverheadShieldBarMarker,
                ));
            });

        // 3. Air Drop Shadow & Tether Line, hidden unless airborne
        builder.spawn((
            Mesh3d(assets.shadow_disc_mesh.clone()),
            MeshMaterial3d(shadow_mat),
            pose.shadow,
            pose.air,
            AirShadowMarker,
        ));
        builder.spawn((
            Mesh3d(assets.tether_line_mesh.clone()),
            MeshMaterial3d(team_mat),
            pose.tether,
            pose.air,
            AirTetherMarker,
        ));
    });
}

/// One indicator entity as [`IndicatorWriter`] writes it.
type IndicatorPart = (
    &'static mut Transform,
    Option<&'static mut Visibility>,
    Option<&'static MeshMaterial3d<SelectionRingMaterial>>,
    Option<&'static mut MeshMaterial3d<StandardMaterial>>,
    Option<&'static Children>,
    (
        Has<GroundRingMarker>,
        Has<OverheadBarRootMarker>,
        Has<OverheadHpBarMarker>,
        Has<AirShadowMarker>,
        Has<AirTetherMarker>,
        Has<OverheadShieldBarMarker>,
    ),
);

/// Writes a reused bubble's indicator children to the [`IndicatorPose`] of its new unit (plan
/// `2026-11-29-render-entity-pool` D-5). Its query reaches indicator entities only: never a live
/// or parked bubble, whose components the live synchroniser writes through its own queries.
#[derive(SystemParam)]
pub struct IndicatorWriter<'w, 's> {
    /// The shared indicator meshes and materials.
    assets: Res<'w, IndicatorAssets>,
    /// Indicator children and grandchildren.
    parts: Query<'w, 's, IndicatorPart, (Without<LiveBattleBubble>, Without<ParkedBattleBubble>)>,
}

/// Sets `value` to `target` only if it differs, so an equal write flags no change.
fn set_if_neq<T: PartialEq + Copy>(value: &mut Mut<'_, T>, target: T) {
    if **value != target {
        **value = target;
    }
}

impl IndicatorWriter<'_, '_> {
    /// Writes every child of `children` (a bubble's) and the bar's grandchildren to `pose`:
    /// transforms, visibilities, the reticle's uniform, the HP bar's full green material and the
    /// tether's team colour. Each material is modified only if it differs.
    pub fn restyle(
        &mut self,
        children: &Children,
        pose: &IndicatorPose,
        materials: &mut Assets<StandardMaterial>,
        ring_materials: &mut Assets<SelectionRingMaterial>,
    ) {
        for &child in children {
            let mut bar_children = [None; 3];
            if let Ok((mut transform, visibility, ring, team, grandchildren, roles)) =
                self.parts.get_mut(child)
            {
                let (is_ring, is_bar, _, is_shadow, is_tether, _) = roles;
                let (target, shown) = if is_ring {
                    (pose.ring, pose.selection)
                } else if is_bar {
                    (pose.bar_root, pose.selection)
                } else if is_shadow {
                    (pose.shadow, pose.air)
                } else if is_tether {
                    (pose.tether, pose.air)
                } else {
                    continue;
                };
                set_if_neq(&mut transform, target);
                if let Some(mut visibility) = visibility {
                    set_if_neq(&mut visibility, shown);
                }
                if let Some(ring) = ring {
                    write_ring(ring_materials, &ring.0, pose.ring_uniform);
                }
                if let (true, Some(team)) = (is_tether, team) {
                    write_team_colour(materials, &team.0, pose.team_color);
                }
                for (slot, grandchild) in bar_children
                    .iter_mut()
                    .zip(grandchildren.into_iter().flatten())
                {
                    *slot = Some(*grandchild);
                }
            }
            for grandchild in bar_children.into_iter().flatten() {
                self.restyle_bar_part(grandchild, pose);
            }
        }
    }

    /// Resets one overhead bar part: the HP bar to full with the green material, the shield bar
    /// to full; the background never changes.
    fn restyle_bar_part(&mut self, part: Entity, pose: &IndicatorPose) {
        let Ok((mut transform, _, _, material, _, roles)) = self.parts.get_mut(part) else {
            return;
        };
        let (_, _, is_hp, _, _, is_shield) = roles;
        if is_hp {
            set_if_neq(&mut transform, pose.hp_bar);
            if let Some(mut material) = material {
                if material.0 != self.assets.hp_green_mat {
                    material.0.clone_from(&self.assets.hp_green_mat);
                }
            }
        } else if is_shield {
            set_if_neq(&mut transform, pose.shield_bar);
        }
    }
}

/// Writes `uniform` into the reticle material `handle` if it differs.
fn write_ring(
    ring_materials: &mut Assets<SelectionRingMaterial>,
    handle: &Handle<SelectionRingMaterial>,
    uniform: SelectionRingUniformGpu,
) {
    if ring_materials
        .get(handle)
        .is_some_and(|material| material.uniform != uniform)
    {
        if let Some(material) = ring_materials.get_mut(handle) {
            material.uniform = uniform;
        }
    }
}

/// Writes the team colour `color` into the tether material `handle` if it differs.
fn write_team_colour(
    materials: &mut Assets<StandardMaterial>,
    handle: &Handle<StandardMaterial>,
    color: Color,
) {
    let target = team_material(color);
    if materials.get(handle).is_some_and(|material| {
        material.base_color != target.base_color || material.emissive != target.emissive
    }) {
        if let Some(material) = materials.get_mut(handle) {
            material.base_color = target.base_color;
            material.emissive = target.emissive;
        }
    }
}

/// Updates visibility, uniform parameters, and scales of selection reticle and overhead bars.
#[expect(
    clippy::type_complexity,
    reason = "a Bevy query declares its whole data access in one type"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn update_indicators_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut exit: EventWriter<'_, AppExit>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    tether_query: Query<
        '_,
        '_,
        &MeshMaterial3d<StandardMaterial>,
        (With<AirTetherMarker>, Without<OverheadHpBarMarker>),
    >,
    lab_state: Option<Res<'_, LabState>>,
    assets: Res<'_, IndicatorAssets>,
    instance_query: Query<'_, '_, (&BubbleInstance, &Children), Without<ParkedBattleBubble>>,
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
    let opt_brackets = lab_state.as_ref().is_none_or(|s| s.opt_selection_brackets);

    for (instance_wrapper, children) in instance_query.iter() {
        let instance = &instance_wrapper.0;
        let style = match owners.palette.resolve(instance.owner) {
            Ok(style) => style,
            Err(error) => {
                owners.reject(error, &mut exit);
                return;
            }
        };
        let is_selected = instance.selected && opt_brackets;
        let selection_vis = if is_selected {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        for &child in children {
            if let Ok(handle) = tether_query.get(child) {
                if let Some(material) = materials.get(&handle.0) {
                    if material.base_color != style.color() {
                        if let Some(material_mut) = materials.get_mut(&handle.0) {
                            material_mut.base_color = style.color();
                            material_mut.emissive = LinearRgba::from(style.color()) * 1.5;
                        }
                    }
                }
            }
            // 1. Update Holographic Ground Reticle Uniform & Visibility
            if let Ok((ring_mat_handle, mut ring_vis)) = ring_query.get_mut(child) {
                if *ring_vis != selection_vis {
                    *ring_vis = selection_vis;
                }
                if let Some(mat) = ring_materials.get(&ring_mat_handle.0) {
                    let target_is_selected = u32::from(is_selected);
                    if mat.uniform.is_selected != target_is_selected
                        || mat.uniform.team_color != style.linear
                    {
                        if let Some(mat_mut) = ring_materials.get_mut(&ring_mat_handle.0) {
                            mat_mut.uniform.is_selected = target_is_selected;
                            mat_mut.uniform.team_color = style.linear;
                        }
                    }
                }
            }

            // 2. Update Floating Overhead Health & Shield Bar Root and its Children
            if let Ok(mut bar_vis) = overhead_query.get_mut(child) {
                if *bar_vis != selection_vis {
                    *bar_vis = selection_vis;
                }

                if is_selected {
                    if let Ok(bar_children) = children_query.get(child) {
                        for &grandchild in bar_children {
                            // Update Health Bar Scale & Dynamic Material Threshold
                            if let Ok((mut hp_tf, mut hp_mat)) = hp_bar_query.get_mut(grandchild) {
                                let clamped_hp = instance.hp_ratio.clamp(0.0, 1.0);
                                if (hp_tf.scale.x - clamped_hp).abs() > 1e-4 {
                                    hp_tf.scale.x = clamped_hp;
                                    hp_tf.translation.x = (clamped_hp - 1.0) * 0.5;
                                }

                                let target_mat = if clamped_hp > 0.50 {
                                    assets.hp_green_mat.clone()
                                } else if clamped_hp > 0.25 {
                                    assets.hp_yellow_mat.clone()
                                } else {
                                    assets.hp_red_mat.clone()
                                };
                                if hp_mat.0 != target_mat {
                                    *hp_mat = MeshMaterial3d(target_mat);
                                }
                            }

                            // Update Shield Bar Scale
                            if let Ok(mut shield_tf) = shield_bar_query.get_mut(grandchild) {
                                let clamped_shield = instance.shield_ratio.clamp(0.0, 1.0);
                                if (shield_tf.scale.x - clamped_shield).abs() > 1e-4 {
                                    shield_tf.scale.x = clamped_shield;
                                    shield_tf.translation.x = (clamped_shield - 1.0) * 0.5;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::enums::{AnimState, SilhouetteClass, WeaponVisualKind};
    use sim_types::{EntityId, FootprintShape, PlayerId, UnitTypeId};

    /// Height of the tether line mesh (`Cylinder::new(0.02, 1.0)`), in its local units.
    const TETHER_MESH_HEIGHT: f32 = 1.0;

    /// Agreement of composed world heights, world units: a few `f32` roundings of values below 16.
    const HEIGHT_TOLERANCE: f32 = 1e-5;

    /// A record of shape `shape`, drawn radius `radius`, `height` above ground `ground_z`.
    fn record(shape: ShapeKind, radius: f32, height: f32, ground_z: f32) -> BubbleRenderInstance {
        BubbleRenderInstance {
            entity_id: EntityId(1),
            shape,
            silhouette: SilhouetteClass::LightProngs,
            unit_type: UnitTypeId(1),
            owner: PlayerId(0),
            world_pos: [3.0, 4.0, ground_z + height],
            ground_z,
            radius,
            velocity: [0.0, 0.0],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Idle,
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            concealment: sim_types::Concealment::None,
            speed_boosted: false,
            stance: sim_types::StanceView::default(),
            status: sim_types::StatusFlags::default(),
            construction_hp_ratio: None,
            barrier_ratio: 0.0,
            housed: false,
        }
    }

    /// The bubble's transform as the live synchroniser writes it (`LiveBubbleFrame::new`,
    /// `update_live_instance`): a sphere's centre one radius above its altitude, a dome's at it.
    fn parent_of(instance: &BubbleRenderInstance) -> Transform {
        let is_dome = matches!(instance.shape, ShapeKind::BuildingDome { .. });
        let lift = if is_dome { 0.0 } else { instance.radius };
        Transform::from_xyz(
            instance.world_pos[0],
            instance.world_pos[2] + lift,
            instance.world_pos[1],
        )
        .with_scale(Vec3::splat(instance.radius))
    }

    fn style() -> crate::player_presentation::PlayerStyle {
        crate::player_presentation::PlayerStyle {
            srgb: [1.0, 1.0, 1.0],
            linear: Vec4::ONE,
            ui: bevy_egui::egui::Color32::WHITE,
        }
    }

    /// U-1 (plan `2026-11-30-terrain-and-air-indicators` §5.3): composed with the bubble's
    /// transform, the shadow lies `SHADOW_LIFT` above the record's ground for every radius,
    /// height, shape and ground, the tether runs from the bubble's centre to the shadow, the air
    /// indicators show exactly when airborne, and no scale is zero.
    #[test]
    fn the_air_shadow_lies_on_the_ground_for_every_radius() {
        let shapes = [
            ShapeKind::UnitSphere,
            ShapeKind::BuildingDome {
                footprint_w: 2,
                footprint_h: 2,
                shape: FootprintShape::full(2, 2),
            },
        ];
        let mut failures = Vec::new();
        for shape in shapes {
            for radius in [0.15, 0.25, 0.4, 1.25] {
                for height in [0.0, 0.4, 0.6, 2.5, 5.0] {
                    for ground_z in [0.0, 3.0, 6.0] {
                        let instance = record(shape, radius, height, ground_z);
                        let parent = parent_of(&instance);
                        let pose = IndicatorPose::of(&instance, style());
                        let shadow = parent.mul_transform(pose.shadow);
                        let tether = parent.mul_transform(pose.tether);
                        let half = 0.5 * TETHER_MESH_HEIGHT * tether.scale.y;
                        let (top, bottom) =
                            (tether.translation.y + half, tether.translation.y - half);
                        let ground = ground_z + SHADOW_LIFT;
                        let case = format!("{shape:?} r {radius} h {height} ground {ground_z}");
                        if (shadow.translation.y - ground).abs() > HEIGHT_TOLERANCE {
                            failures.push(format!(
                                "{case}: shadow at {}, ground + lift {ground}",
                                shadow.translation.y
                            ));
                        }
                        if (top - parent.translation.y).abs() > HEIGHT_TOLERANCE
                            || (bottom - shadow.translation.y).abs() > HEIGHT_TOLERANCE
                        {
                            failures.push(format!(
                                "{case}: tether {bottom}..{top}, centre {}, shadow {}",
                                parent.translation.y, shadow.translation.y
                            ));
                        }
                        let shown = pose.air != Visibility::Hidden;
                        if shown != (height > AIRBORNE_HEIGHT) {
                            failures.push(format!("{case}: air indicators shown = {shown}"));
                        }
                        for scale in [pose.shadow.scale, pose.tether.scale] {
                            if scale.cmpeq(Vec3::ZERO).any() {
                                failures.push(format!("{case}: zero scale {scale}"));
                            }
                        }
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} air pose failures, e.g.:\n{}",
            failures.len(),
            failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
