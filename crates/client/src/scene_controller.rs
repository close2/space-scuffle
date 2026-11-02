//! Simulation, preview, and swarm scene controller systems for the Space Scuffle client.
//!
//! # Architectural Responsibilities
//! This module decouples Bevy ECS simulation synchronization and 3D scene lifecycle management
//! from `egui` UI presentation:
//! - **4-Hero Visual Lab Preview:** Dynamically queries `CatalogResource` for hero showcase archetypes,
//!   animating and rendering them with zero hardcoded unit IDs or creature names.
//! - **Live Combat Sandbox Synchronizer:** Applies immutable displayed snapshots and synchronizes
//!   3D Bevy entities using retained snapshots and persistent [`RenderEntityMap`] storage.
//! - **200-Bubble Swarm Stress Test:** High-density collision and rendering stress test.
//!
//! # Invariants
//! - Conversion from deterministic fixed-point to GPU floats happens strictly at snapshot boundaries.
//! - Live steady-state synchronization reuses storage; spawn/despawn and preview work have separate costs.
//! - Components (`Transform`, `BubbleInstance`) are mutated in place without issuing `commands.insert`.

use crate::bubble_shader::{
    sync_bubble_material, BubbleDomeMaterial, BubbleInstance, BubbleSharedMeshes, BubbleUniformGpu,
    FlipbookRegistry, FLAG_ATTACK_APERTURE, FLAG_CLOAKED, FLAG_CRACKS, FLAG_CRT_GLITCH,
    FLAG_FACING_STRIPE, FLAG_FRESNEL, FLAG_GLARE, FLAG_INNER_WARP, FLAG_SELECTED,
    FLAG_SPEED_BOOSTED,
};
use crate::indicators::{attach_bubble_indicators, IndicatorAssets, SelectionRingMaterial};
use crate::lab_ui::{LabState, LabTab};
use crate::selection::{HeroLineupIndex, SelectionState};
use crate::CatalogResource;
use bevy::app::AppExit;
use bevy::prelude::*;
use sim_core::catalog::{UnitArchetype, UnitCatalog};
use sim_types::enums::{AnimState, ShapeKind, WeaponVisualKind};
use sim_types::ids::{EntityId, PlayerId, UnitTypeId};
use sim_types::snapshot::BubbleRenderInstance;
use std::collections::BTreeMap;

/// Marker component for close-up hero showcase preview bubbles in the Visual Lab.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabPreviewBubble;

/// Marker component for live battle simulation bubble entities.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveBattleBubble {
    /// Authoritative simulation `EntityId`.
    pub entity_id: EntityId,
}

/// Marker component for swarm stress test bubble entities.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwarmBubble;

/// Persistent mapping from authoritative simulation [`EntityId`] to Bevy render [`Entity`].
///
/// PERFORMANCE RATIONALE:
/// Why: Allocating a `HashMap` on every frame produces GC-like cache churn and memory fragmentation.
/// What: We retain a persistent map in a Bevy resource, inserting only when entities spawn and
/// removing only when entities despawn, resulting in zero allocations during steady-state ticks.
/// Trade-off: Minimal persistent resource state in exchange for zero heap allocations per frame.
#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub struct RenderEntityMap(pub BTreeMap<EntityId, Entity>);

/// Catalog identities flagged `showcase_hero`, sorted by `showcase_order`.
///
// PERFORMANCE RATIONALE:
// Why: The preview synchronizer and the unit HUD both need the lineup on every frame.
// What: Each keeps one sorted list in system-local state and rebuilds it when the catalog changes.
// Trade-off: A catalog lookup per actor in exchange for no per-frame allocation or sort.
#[derive(Debug, Default)]
pub struct ShowcaseLineup {
    ids: Vec<UnitTypeId>,
}

impl ShowcaseLineup {
    /// Rebuilds the lineup from `catalog` if `catalog_changed`, reusing its storage.
    pub fn refresh(&mut self, catalog: &UnitCatalog, catalog_changed: bool) {
        if !catalog_changed {
            return;
        }
        self.ids.clear();
        self.ids.extend(
            catalog
                .archetypes()
                .filter(|archetype| archetype.showcase_hero)
                .map(|archetype| archetype.unit_type_id),
        );
        self.ids.sort_by_key(|id| {
            catalog
                .get(*id)
                .ok()
                .and_then(|archetype| archetype.showcase_order)
                .unwrap_or(u8::MAX)
        });
    }

    /// Number of identities in the lineup.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the catalog flags no showcase identity.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Identities in lineup order.
    #[must_use]
    pub fn ids(&self) -> &[UnitTypeId] {
        &self.ids
    }

    /// Archetypes in lineup order. An identity missing from `catalog` is skipped.
    pub fn archetypes<'a>(
        &'a self,
        catalog: &'a UnitCatalog,
    ) -> impl Iterator<Item = &'a UnitArchetype> + 'a {
        self.ids.iter().filter_map(|id| catalog.get(*id).ok())
    }
}

/// World position and radius of lineup slot `index`, in world units.
fn preview_placement(index: usize, is_dome: bool) -> (Vec3, f32) {
    let x = match index {
        0 => -3.6,
        1 => -1.2,
        2 => 1.2,
        _ => 4.2,
    };
    let radius = if is_dome { 1.35 } else { 0.85 };
    let y = if is_dome { 0.0 } else { radius };
    (Vec3::new(x, y, 0.0), radius)
}

/// Builds the snapshot of one preview actor. Lab controls act on the selected actor only.
///
/// `elapsed_seconds` is presentation time, and `pos` is in Bevy world space.
#[expect(
    clippy::cast_possible_truncation,
    reason = "presentation values are bounded by frame, atlas and window sizes"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "explicit inputs keep this helper free of hidden state"
)]
fn preview_instance(
    lab: &LabState,
    archetype: &UnitArchetype,
    index: usize,
    owner: PlayerId,
    pos: Vec3,
    radius: f32,
    is_selected: bool,
    elapsed_seconds: f32,
) -> BubbleRenderInstance {
    let selected_or = |value: f32, otherwise: f32| if is_selected { value } else { otherwise };
    let anim_state = if is_selected {
        lab.anim_state
    } else {
        AnimState::Idle
    };
    BubbleRenderInstance {
        entity_id: EntityId(100 + index as u32),
        shape: archetype.shape,
        silhouette: archetype.silhouette,
        unit_type: archetype.unit_type_id,
        owner,
        world_pos: [pos.x, pos.z, 0.0],
        ground_z: 0.0,
        radius,
        velocity: [selected_or(lab.speed, 0.0), 0.0],
        heading_angle_rad: selected_or(lab.heading_angle_deg.to_radians(), 0.0),
        aim_angle_rad: selected_or(lab.aim_angle_deg.to_radians(), 0.0),
        is_attacking: is_selected && lab.anim_state == AnimState::Attack,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state,
        anim_progress: (elapsed_seconds * 1.5).fract(),
        hp_ratio: selected_or(lab.hp_ratio, 1.0),
        shield_ratio: selected_or(lab.shield_ratio, 1.0),
        shield_hit_flash: selected_or(lab.shield_hit_flash, 0.0),
        hull_hit_angle_rad: selected_or(lab.hit_angle_deg.to_radians(), 0.0),
        hull_hit_flash: selected_or(lab.hull_hit_flash, 0.0),
        build_or_queue_progress: Some(selected_or(lab.build_progress, 1.0)),
        selected: is_selected,
        cloaked: is_selected && lab.cloaked,
        speed_boosted: is_selected && lab.speed_boosted,
    }
}

/// Packs the Lab's optical toggles and the actor's own state into shader feature flags.
fn preview_feature_flags(lab: &LabState, instance: &BubbleRenderInstance, selected: bool) -> u32 {
    [
        (lab.opt_glare, FLAG_GLARE),
        (lab.opt_fresnel, FLAG_FRESNEL),
        (lab.opt_inner_warp, FLAG_INNER_WARP),
        (lab.opt_cracks, FLAG_CRACKS),
        (lab.opt_crt_glitch, FLAG_CRT_GLITCH),
        (lab.opt_facing_chevron, FLAG_FACING_STRIPE),
        (lab.opt_focal_lens, FLAG_ATTACK_APERTURE),
        (selected, FLAG_SELECTED),
        (instance.cloaked, FLAG_CLOAKED),
        (instance.speed_boosted, FLAG_SPEED_BOOSTED),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .fold(0, |flags, (_, flag)| flags | flag)
}

/// Builds the snapshot of one walking swarm bubble.
///
/// `world_xz` is in Bevy world units, and `speed_x` is in world units per second.
fn swarm_instance(
    archetype: &UnitArchetype,
    entity_id: u32,
    owner: PlayerId,
    world_xz: Vec2,
    speed_x: f32,
) -> BubbleRenderInstance {
    BubbleRenderInstance {
        entity_id: EntityId(entity_id),
        shape: ShapeKind::UnitSphere,
        silhouette: archetype.silhouette,
        unit_type: archetype.unit_type_id,
        owner,
        world_pos: [world_xz.x, world_xz.y, 0.5],
        ground_z: 0.0,
        radius: 0.45,
        velocity: [speed_x, 0.0],
        heading_angle_rad: if speed_x < 0.0 {
            std::f32::consts::PI
        } else {
            0.0
        },
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: AnimState::Walk,
        anim_progress: 0.0,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        cloaked: false,
        speed_boosted: false,
    }
}

/// Synchronizes the 4-Hero Showcase Lineup in the Visual Lab with GUI state and selection.
///
/// Queries `CatalogResource` dynamically for all archetypes flagged with `showcase_hero: true`
/// sorted by `showcase_order`. Contains zero hardcoded creature names or unit type IDs.
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
#[expect(
    clippy::too_many_lines,
    reason = "a single ordered sequence that shares its local state"
)]
pub fn sync_lab_preview_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut commands: Commands<'_, '_>,
    mut exit_events: EventWriter<'_, AppExit>,
    time: Res<'_, Time>,
    mut lab_state: ResMut<'_, LabState>,
    mut selection_state: ResMut<'_, SelectionState>,
    catalog: Res<'_, CatalogResource>,
    mut lineup: Local<'_, ShowcaseLineup>,
    registry: Res<'_, FlipbookRegistry>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    indicator_assets: Res<'_, IndicatorAssets>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>,
    mut query: Query<
        '_,
        '_,
        (
            Entity,
            &MeshMaterial3d<BubbleDomeMaterial>,
            &HeroLineupIndex,
            &mut BubbleInstance,
            &mut Transform,
            &mut Visibility,
        ),
        With<LabPreviewBubble>,
    >,
) {
    if lab_state.current_tab != LabTab::VisualLab {
        // Despawn preview bubbles when switching tabs
        for (entity, _, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        return;
    }

    // Decay damage flash intensities
    let dt = time.delta_secs();
    lab_state.shield_hit_flash = (lab_state.shield_hit_flash - dt * 3.0).max(0.0);
    lab_state.hull_hit_flash = (lab_state.hull_hit_flash - dt * 3.0).max(0.0);

    lineup.refresh(&catalog.0, catalog.is_changed());

    let hero_count = query.iter().count();
    let expected_heroes = lineup.len().min(4);

    if media_status.retain_failure(&mut lab_state) {
        return;
    }
    let selected = selection_state
        .selected_hero_index
        .unwrap_or(lab_state.selected_hero_index)
        .min(expected_heroes.saturating_sub(1));
    // Admit the entire requested lineup before creating any material. Successful
    // lookups use cached startup evidence and never touch files or allocate.
    for (index, arch) in lineup
        .archetypes(&catalog.0)
        .take(expected_heroes)
        .enumerate()
    {
        let action = if index == selected {
            lab_state.anim_state
        } else {
            AnimState::Idle
        };
        let admission = media_status
            .require(&registry, arch.unit_type_id, action)
            .and_then(|()| {
                if index == selected && lab_state.use_concept_video {
                    media_status.require_concept(&registry, action)
                } else {
                    Ok(())
                }
            });
        if let Err(error) = admission {
            crate::media::reject(error, &mut media_status, &mut lab_state, &mut exit_events);
            return;
        }
    }

    if hero_count < expected_heroes {
        for (entity, _, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }

        for (idx, arch) in lineup.archetypes(&catalog.0).take(4).enumerate() {
            let is_dome = matches!(arch.shape, ShapeKind::BuildingDome { .. });
            let is_selected = idx == lab_state.selected_hero_index;

            let (pos, radius) = preview_placement(idx, is_dome);
            let owner = owners.palette.showcase(idx);

            let texture = registry.get_texture(arch.unit_type_id);
            let meta = registry.get_meta(arch.unit_type_id);

            let elapsed = time.elapsed_secs();
            let instance = preview_instance(
                &lab_state,
                arch,
                idx,
                owner,
                pos,
                radius,
                is_selected,
                elapsed,
            );

            let style = match owners.palette.preview_style(instance.owner) {
                Ok(style) => style,
                Err(error) => {
                    owners.reject(error, &mut exit_events);
                    return;
                }
            };
            let mut uniform = BubbleUniformGpu::from_instance(&instance, meta, style);
            uniform.feature_flags = preview_feature_flags(&lab_state, &instance, is_selected);

            let mat = bubble_materials.add(BubbleDomeMaterial {
                uniform,
                video_texture: texture,
                alpha_mode: AlphaMode::Blend,
            });

            let mesh = if is_dome {
                shared_meshes.dome_mesh.clone()
            } else {
                shared_meshes.sphere_mesh.clone()
            };

            let show_preview_heroes =
                !(lab_state.smog_bouncing_active && lab_state.opt_smog_volume);
            let preview_vis = if show_preview_heroes {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };

            let entity = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_translation(pos).with_scale(Vec3::splat(radius)),
                    preview_vis,
                    BubbleInstance(instance.clone()),
                    LabPreviewBubble,
                    HeroLineupIndex(idx),
                ))
                .id();

            if is_selected && selection_state.selected_entity.is_none() {
                selection_state.select(entity, Some(instance.entity_id), Some(idx));
            }

            attach_bubble_indicators(
                &mut commands,
                entity,
                &instance,
                &indicator_assets,
                &mut materials,
                &mut ring_materials,
                style,
            );
        }
        return;
    }

    // If already spawned, update each hero in place.
    let active_hero_idx = selection_state
        .selected_hero_index
        .unwrap_or(lab_state.selected_hero_index)
        .min(expected_heroes.saturating_sub(1));
    lab_state.selected_hero_index = active_hero_idx;

    let show_preview_heroes = !(lab_state.smog_bouncing_active && lab_state.opt_smog_volume);
    let target_vis = if show_preview_heroes {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    for (_entity, mat_handle, hero_idx, mut bubble_instance, mut transform, mut vis) in &mut query {
        if *vis != target_vis {
            *vis = target_vis;
        }
        let is_selected = hero_idx.0 == active_hero_idx;
        let Some(arch) = lineup
            .ids()
            .get(hero_idx.0)
            .and_then(|id| catalog.0.get(*id).ok())
        else {
            continue;
        };

        let is_dome = matches!(arch.shape, ShapeKind::BuildingDome { .. });
        let (pos, radius) = preview_placement(hero_idx.0, is_dome);
        let owner = owners.palette.showcase(hero_idx.0);

        let (texture, meta) = if is_selected && lab_state.use_concept_video {
            (
                registry.get_texture_by_name("concept_video"),
                registry.get_meta_by_name("concept_video"),
            )
        } else {
            (
                registry.get_texture(arch.unit_type_id),
                registry.get_meta(arch.unit_type_id),
            )
        };

        let instance = preview_instance(
            &lab_state,
            arch,
            hero_idx.0,
            owner,
            pos,
            radius,
            is_selected,
            time.elapsed_secs(),
        );

        let style = match owners.palette.preview_style(instance.owner) {
            Ok(style) => style,
            Err(error) => {
                owners.reject(error, &mut exit_events);
                return;
            }
        };
        let mut uniform = BubbleUniformGpu::from_instance(&instance, meta, style);
        uniform.feature_flags = preview_feature_flags(&lab_state, &instance, is_selected);

        sync_bubble_material(&mut bubble_materials, &mat_handle.0, &uniform, texture);

        // Writing through `Mut` flags change detection even for an equal value.
        transform.set_if_neq(
            transform
                .with_translation(pos)
                .with_scale(Vec3::splat(radius)),
        );

        // Mutate component in place without issuing commands.entity(e).insert(...)
        bubble_instance.0 = instance;
    }
}

type LiveBattleQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static mut BubbleInstance,
        &'static mut Mesh3d,
        &'static MeshMaterial3d<BubbleDomeMaterial>,
    ),
    With<LiveBattleBubble>,
>;

/// Applies the retained displayed snapshot; never executes simulation or consumes output.
/// Spatial transforms and material instances use exactly the same published pose.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn sync_live_battle_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut commands: Commands<'_, '_>,
    mut exit_events: EventWriter<'_, AppExit>,
    lab_state: Res<'_, LabState>,
    session: Res<'_, crate::battle_pipeline::session::BattleSession>,
    mut render_entities: ResMut<'_, RenderEntityMap>,
    registry: Res<'_, FlipbookRegistry>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    indicator_assets: Res<'_, IndicatorAssets>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>,
    mut query: LiveBattleQuery<'_, '_>,
) {
    if lab_state.current_tab != LabTab::LiveBattle {
        for (entity, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        render_entities.0.clear();
        return;
    }
    if !session.is_ready() {
        return;
    }
    let snapshot = session.frames.presented();

    // Reuse existing materials and instances; spawn/despawn are separate lifecycle work.
    for instance in &snapshot.bubbles {
        let meta = registry.get_meta(instance.unit_type);
        let texture = registry.get_texture(instance.unit_type);
        let style = match owners
            .palette
            .battle_style(instance.owner, &session.metadata)
        {
            Ok(style) => style,
            Err(error) => {
                owners.reject(error, &mut exit_events);
                return;
            }
        };
        let uniform = BubbleUniformGpu::from_instance(instance, meta, style);
        let is_dome = matches!(instance.shape, ShapeKind::BuildingDome { .. });
        let altitude_offset = if is_dome { 0.0 } else { instance.radius };
        let pos = Vec3::new(
            instance.world_pos[0],
            instance.world_pos[2] + altitude_offset,
            instance.world_pos[1],
        );

        let target_pos = pos;
        if let Some(&e) = render_entities.0.get(&instance.entity_id) {
            if let Ok((_, mut transform, mut bubble, mut mesh, mat_handle)) = query.get_mut(e) {
                sync_bubble_material(&mut bubble_materials, &mat_handle.0, &uniform, texture);
                // Writing through `Mut` flags change detection even for an equal value, and a
                // changed `Mesh3d` makes Bevy re-extract the instance.
                transform.set_if_neq(
                    transform
                        .with_translation(target_pos)
                        .with_scale(Vec3::splat(instance.radius)),
                );
                let target_mesh = if is_dome {
                    &shared_meshes.dome_mesh
                } else {
                    &shared_meshes.sphere_mesh
                };
                if mesh.0 != *target_mesh {
                    mesh.0.clone_from(target_mesh);
                }
                bubble.0 = instance.clone();
            }
        } else {
            let mat = bubble_materials.add(BubbleDomeMaterial {
                uniform,
                video_texture: texture,
                alpha_mode: AlphaMode::Blend,
            });

            let mesh = if is_dome {
                shared_meshes.dome_mesh.clone()
            } else {
                shared_meshes.sphere_mesh.clone()
            };

            let entity = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_translation(pos).with_scale(Vec3::splat(instance.radius)),
                    BubbleInstance(instance.clone()),
                    LiveBattleBubble {
                        entity_id: instance.entity_id,
                    },
                ))
                .id();

            render_entities.0.insert(instance.entity_id, entity);

            attach_bubble_indicators(
                &mut commands,
                entity,
                instance,
                &indicator_assets,
                &mut materials,
                &mut ring_materials,
                style,
            );
        }
    }

    // PERFORMANCE RATIONALE:
    // Why: Scanning the entire snapshot for each renderer is quadratic.
    // What: Merge the sorted entity map and sorted snapshot membership once.
    // Trade-off: Requires the authoritative snapshot's stable ID ordering.
    let mut live = snapshot.bubbles.iter().peekable();
    render_entities.0.retain(|&id, &mut entity| {
        while live.peek().is_some_and(|record| record.entity_id < id) {
            live.next();
        }
        let is_alive = live.peek().is_some_and(|record| record.entity_id == id);
        if !is_alive {
            commands.entity(entity).despawn_recursive();
        }
        is_alive
    });
}

/// Smallest world X a swarm bubble centre reaches before bouncing.
pub const SWARM_MIN_X: f32 = 13.0;

/// Largest world X a swarm bubble centre reaches before bouncing.
pub const SWARM_MAX_X: f32 = 31.0;

/// World Z of the swarm's first row. All ten rows stay on open low ground, south of the ramp.
pub const SWARM_MIN_Z: f32 = 3.6;

/// World units between neighbouring swarm rows and columns.
const SWARM_SPACING: f32 = 1.2;

/// World Z of the swarm's last row.
pub const SWARM_MAX_Z: f32 = SWARM_MIN_Z + 9.0 * SWARM_SPACING;

/// World `(x, z)` spawn position of swarm bubble `index`, where `index` is below 200.
#[expect(
    clippy::cast_precision_loss,
    reason = "presentation values are bounded by frame, atlas and window sizes"
)]
fn swarm_spawn_position(index: u32) -> Vec2 {
    let row = (index % 10) as f32;
    let column = ((index % 100) / 10) as f32;
    let x = if index < 100 {
        SWARM_MIN_X + column * SWARM_SPACING
    } else {
        SWARM_MAX_X - column * SWARM_SPACING
    };
    Vec2::new(x, SWARM_MIN_Z + row * SWARM_SPACING)
}

/// Spawns and animates 200 colliding bubbles for the swarm stress test.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation values are bounded by frame, atlas and window sizes"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
#[expect(
    clippy::too_many_lines,
    reason = "a single ordered sequence that shares its local state"
)]
pub fn sync_swarm_stress_system(
    mut owners: crate::player_presentation::PresentationContext<'_>,
    mut commands: Commands<'_, '_>,
    mut exit_events: EventWriter<'_, AppExit>,
    time: Res<'_, Time>,
    mut lab_state: ResMut<'_, LabState>,
    catalog: Res<'_, CatalogResource>,
    registry: Res<'_, FlipbookRegistry>,
    mut media_status: ResMut<'_, crate::media::ClientMediaStatus>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut camera_query: Query<'_, '_, &mut crate::camera::RtsCamera>,
    mut query: Query<
        '_,
        '_,
        (
            Entity,
            &mut BubbleInstance,
            &mut Transform,
            &MeshMaterial3d<BubbleDomeMaterial>,
        ),
        With<SwarmBubble>,
    >,
) {
    if lab_state.current_tab != LabTab::SwarmStressTest || !lab_state.swarm_active {
        // Despawn swarm when inactive
        for (entity, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        lab_state.swarm_bubble_count = 0;
        return;
    }

    if media_status.retain_failure(&mut lab_state) {
        return;
    }
    let Some(mobile_arch) = catalog
        .0
        .archetypes()
        .find(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
    else {
        return;
    };
    if let Err(error) = media_status.require(&registry, mobile_arch.unit_type_id, AnimState::Walk) {
        crate::media::reject(error, &mut media_status, &mut lab_state, &mut exit_events);
        return;
    }
    let count = query.iter().count();
    lab_state.swarm_bubble_count = count;

    if count == 0 {
        let meta = registry.get_meta(mobile_arch.unit_type_id);
        let texture = registry.get_texture(mobile_arch.unit_type_id);

        if let Some(mut cam) = camera_query.iter_mut().next() {
            cam.target = Vec2::new(
                f32::midpoint(SWARM_MIN_X, SWARM_MAX_X),
                f32::midpoint(SWARM_MIN_Z, SWARM_MAX_Z),
            );
            cam.look_at_y = 0.5;
            cam.distance = 26.0;
        }

        // Interactive 200-unit Swarm Stress Test:
        // Share only 2 Handle<BubbleDomeMaterial> instances (1 for primary 100, 1 for secondary 100)
        // to enable Bevy GPU batching and cut BindGroup recreations from 200/frame to 2/frame.
        let primary_owner = owners.palette.primary();
        let secondary_owner = owners.palette.secondary();
        let primary_style = match owners.palette.preview_style(primary_owner) {
            Ok(s) => s,
            Err(error) => {
                owners.reject(error, &mut exit_events);
                return;
            }
        };
        let secondary_style = match owners.palette.preview_style(secondary_owner) {
            Ok(s) => s,
            Err(error) => {
                owners.reject(error, &mut exit_events);
                return;
            }
        };

        let primary_dummy_instance =
            swarm_instance(mobile_arch, 1000, primary_owner, Vec2::new(8.0, 10.0), 1.5);
        let primary_uniform =
            BubbleUniformGpu::from_instance(&primary_dummy_instance, meta, primary_style);
        let primary_mat = bubble_materials.add(BubbleDomeMaterial {
            uniform: primary_uniform,
            video_texture: texture.clone(),
            alpha_mode: AlphaMode::Blend,
        });

        let secondary_dummy_instance = swarm_instance(
            mobile_arch,
            1100,
            secondary_owner,
            Vec2::new(24.0, 10.0),
            -1.5,
        );
        let secondary_uniform =
            BubbleUniformGpu::from_instance(&secondary_dummy_instance, meta, secondary_style);
        let secondary_mat = bubble_materials.add(BubbleDomeMaterial {
            uniform: secondary_uniform,
            video_texture: texture.clone(),
            alpha_mode: AlphaMode::Blend,
        });

        for i in 0..200u32 {
            let primary_group = i < 100;
            let owner = if primary_group {
                primary_owner
            } else {
                secondary_owner
            };
            let Vec2 { x, y: z } = swarm_spawn_position(i);

            let speed_x = if primary_group { 1.5 } else { -1.5 };
            let instance = swarm_instance(mobile_arch, 1000 + i, owner, Vec2::new(x, z), speed_x);

            let mat = if primary_group {
                primary_mat.clone()
            } else {
                secondary_mat.clone()
            };

            commands.spawn((
                Mesh3d(shared_meshes.sphere_mesh.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz(x, 0.5, z).with_scale(Vec3::splat(0.45)),
                BubbleInstance(instance),
                SwarmBubble,
            ));
        }
        lab_state.swarm_bubble_count = 200;
    } else {
        // Interactive 200-unit Swarm Stress Test:
        // Update 200 entity transforms/velocities with map-plane bouncing,
        // while mutating only the 2 shared BubbleDomeMaterial handles!
        let dt = time.delta_secs();
        let anim_progress = (time.elapsed_secs() * 2.0).fract();
        for (_, mut instance, mut tf, mat_handle) in &mut query {
            let style = match owners.palette.preview_style(instance.0.owner) {
                Ok(style) => style,
                Err(error) => {
                    owners.reject(error, &mut exit_events);
                    return;
                }
            };
            let mut vx = instance.0.velocity[0];
            tf.translation.x += vx * dt;
            if tf.translation.x <= SWARM_MIN_X {
                tf.translation.x = SWARM_MIN_X;
                vx = vx.abs();
                instance.0.velocity[0] = vx;
                instance.0.heading_angle_rad = 0.0;
            } else if tf.translation.x >= SWARM_MAX_X {
                tf.translation.x = SWARM_MAX_X;
                vx = -vx.abs();
                instance.0.velocity[0] = vx;
                instance.0.heading_angle_rad = std::f32::consts::PI;
            }
            instance.0.world_pos = [tf.translation.x, tf.translation.z, tf.translation.y];

            let new_frame = if let Some(meta) = registry.get_meta(instance.0.unit_type) {
                if let Some(slice) = meta.actions.get(&instance.0.anim_state) {
                    if slice.frame_count == 0 {
                        slice.start_frame
                    } else if slice.looping {
                        let step = (anim_progress * slice.frame_count as f32) as u32;
                        slice.start_frame + (step % slice.frame_count)
                    } else {
                        let step = (anim_progress * slice.frame_count as f32) as u32;
                        slice.start_frame + step.min(slice.frame_count.saturating_sub(1))
                    }
                } else {
                    0
                }
            } else {
                0
            };

            // Checking the material first ensures that out of 200 entities sharing 2 handles,
            // only the first entity of each group triggers get_mut; the remaining 99 check get() and skip in O(1)!
            if let Some(mat) = bubble_materials.get(&mat_handle.0) {
                if mat.uniform.team_color != style.linear
                    || mat.uniform.frame_index != new_frame
                    || (mat.uniform.heading_angle_rad - instance.0.heading_angle_rad).abs() > 1e-4
                {
                    if let Some(mat_mut) = bubble_materials.get_mut(&mat_handle.0) {
                        mat_mut.uniform.team_color = style.linear;
                        mat_mut.uniform.frame_index = new_frame;
                        mat_mut.uniform.anim_progress = anim_progress;
                        mat_mut.uniform.heading_angle_rad = instance.0.heading_angle_rad;
                    }
                }
            }
        }
    }
}
