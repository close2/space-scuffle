//! Simulation, preview, and swarm scene controller systems for the Space Scuffle client.
//!
//! # Architectural Responsibilities
//! This module decouples Bevy ECS simulation synchronization and 3D scene lifecycle management
//! from `egui` UI presentation:
//! - **4-Hero Visual Lab Preview:** Dynamically queries `CatalogResource` for hero showcase archetypes,
//!   animating and rendering them with zero hardcoded unit IDs or creature names.
//! - **Live Combat Sandbox Synchronizer:** Steps authoritative `sim_core::SimWorld` and synchronizes
//!   3D Bevy entities with zero per-frame heap allocations using persistent [`RenderEntityMap`].
//! - **200-Bubble Swarm Stress Test:** High-density collision and rendering stress test.
//!
//! # Invariants
//! - Conversion from deterministic fixed-point to GPU floats happens strictly at snapshot boundaries.
//! - Per-frame sync systems perform zero heap allocations in steady state.
//! - Components (`Transform`, `BubbleInstance`) are mutated in place without issuing `commands.insert`.

use crate::bubble_shader::{
    BubbleDomeMaterial, BubbleInstance, BubbleSharedMeshes, BubbleUniformGpu, FlipbookRegistry,
    FLAG_ATTACK_APERTURE, FLAG_CLOAKED, FLAG_CRACKS, FLAG_CRT_GLITCH, FLAG_FACING_STRIPE,
    FLAG_FRESNEL, FLAG_GLARE, FLAG_INNER_WARP, FLAG_SELECTED, FLAG_SPEED_BOOSTED,
};
use crate::indicators::{attach_bubble_indicators, IndicatorAssets, SelectionRingMaterial};
use crate::lab_ui::{LabState, LabTab};
use crate::selection::{HeroLineupIndex, SelectionState};
use crate::vfx::{emit_mist_wake, CombatVisualEvent, MistWakeMaterial, VfxSharedAssets};
use crate::CatalogResource;
use bevy::prelude::*;
use sim_core::world::SimWorld;
use sim_types::commands::TickBundle;
use sim_types::enums::{AnimState, ShapeKind, WeaponVisualKind};
use sim_types::ids::{EntityId, TeamId, Tick};
use sim_types::snapshot::BubbleRenderInstance;
use std::collections::BTreeMap;

/// Marker component for close-up hero showcase preview bubbles in the Visual Lab.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabPreviewBubble;

/// Marker component for live battle simulation bubble entities.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveBattleBubble {
    /// Authoritative simulation EntityId.
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

/// Synchronizes the 4-Hero Showcase Lineup in the Visual Lab with GUI state and selection.
///
/// Queries `CatalogResource` dynamically for all archetypes flagged with `showcase_hero: true`
/// sorted by `showcase_order`. Contains zero hardcoded creature names or unit type IDs.
#[allow(clippy::type_complexity)]
pub fn sync_lab_preview_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    mut lab_state: ResMut<'_, LabState>,
    mut selection_state: ResMut<'_, SelectionState>,
    catalog: Res<'_, CatalogResource>,
    registry: Res<'_, FlipbookRegistry>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    indicator_assets: Res<'_, IndicatorAssets>,
    vfx_assets: Res<'_, VfxSharedAssets>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>,
    mut mist_materials: ResMut<'_, Assets<MistWakeMaterial>>,
    mut query: Query<
        '_,
        '_,
        (
            Entity,
            &MeshMaterial3d<BubbleDomeMaterial>,
            &HeroLineupIndex,
            &mut BubbleInstance,
            &mut Transform,
        ),
        With<LabPreviewBubble>,
    >,
) {
    if lab_state.current_tab != LabTab::VisualLab {
        // Despawn preview bubbles when switching tabs
        for (entity, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        return;
    }

    // Decay damage flash intensities
    let dt = time.delta_secs();
    lab_state.shield_hit_flash = (lab_state.shield_hit_flash - dt * 3.0).max(0.0);
    lab_state.hull_hit_flash = (lab_state.hull_hit_flash - dt * 3.0).max(0.0);

    // Filter and sort showcase heroes dynamically from catalog
    let mut heroes: Vec<&sim_core::catalog::UnitArchetype> =
        catalog.0.archetypes().filter(|a| a.showcase_hero).collect();
    heroes.sort_by_key(|a| a.showcase_order.unwrap_or(u8::MAX));

    let hero_count = query.iter().count();
    let expected_heroes = heroes.len().min(4);

    if hero_count < expected_heroes {
        for (entity, _, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }

        for (idx, &arch) in heroes.iter().take(4).enumerate() {
            let is_dome = matches!(arch.shape, ShapeKind::BuildingDome { .. });
            let is_selected = idx == lab_state.selected_hero_index;

            let x = match idx {
                0 => -3.6,
                1 => -1.2,
                2 => 1.2,
                _ => 4.2,
            };
            let radius = if is_dome { 1.35 } else { 0.85 };
            let y = if is_dome { 0.0 } else { radius };
            let pos = Vec3::new(x, y, 0.0);
            let team_id = if idx == 2 { TeamId(2) } else { TeamId(1) };

            let texture = registry.get_texture(arch.unit_type_id);
            let meta = registry.get_meta(arch.unit_type_id);

            let instance = BubbleRenderInstance {
                entity_id: EntityId(100 + idx as u32),
                shape: arch.shape,
                silhouette: arch.silhouette,
                unit_type: arch.unit_type_id,
                team_id,
                world_pos: [pos.x, pos.z, 0.0],
                ground_z: 0.0,
                radius,
                velocity: if is_selected {
                    [lab_state.speed, 0.0]
                } else {
                    [0.0, 0.0]
                },
                heading_angle_rad: if is_selected {
                    lab_state.heading_angle_deg.to_radians()
                } else {
                    0.0
                },
                aim_angle_rad: if is_selected {
                    lab_state.aim_angle_deg.to_radians()
                } else {
                    0.0
                },
                is_attacking: is_selected && lab_state.anim_state == AnimState::Attack,
                weapon_visual: WeaponVisualKind::Hitscan,
                attack_target_pos: None,
                anim_state: if is_selected {
                    lab_state.anim_state
                } else {
                    AnimState::Idle
                },
                anim_progress: (time.elapsed_secs() * 1.5).fract(),
                hp_ratio: if is_selected { lab_state.hp_ratio } else { 1.0 },
                shield_ratio: if is_selected {
                    lab_state.shield_ratio
                } else {
                    1.0
                },
                shield_hit_flash: if is_selected {
                    lab_state.shield_hit_flash
                } else {
                    0.0
                },
                hull_hit_angle_rad: if is_selected {
                    lab_state.hit_angle_deg.to_radians()
                } else {
                    0.0
                },
                hull_hit_flash: if is_selected {
                    lab_state.hull_hit_flash
                } else {
                    0.0
                },
                build_or_queue_progress: Some(if is_selected {
                    lab_state.build_progress
                } else {
                    1.0
                }),
                selected: is_selected,
                cloaked: is_selected && lab_state.cloaked,
                speed_boosted: is_selected && lab_state.speed_boosted,
            };

            let mut uniform = BubbleUniformGpu::from_instance(&instance, meta);
            let mut feature_flags = 0u32;
            if lab_state.opt_glare {
                feature_flags |= FLAG_GLARE;
            }
            if lab_state.opt_fresnel {
                feature_flags |= FLAG_FRESNEL;
            }
            if lab_state.opt_inner_warp {
                feature_flags |= FLAG_INNER_WARP;
            }
            if lab_state.opt_cracks {
                feature_flags |= FLAG_CRACKS;
            }
            if lab_state.opt_crt_glitch {
                feature_flags |= FLAG_CRT_GLITCH;
            }
            if lab_state.opt_facing_chevron {
                feature_flags |= FLAG_FACING_STRIPE;
            }
            if lab_state.opt_focal_lens {
                feature_flags |= FLAG_ATTACK_APERTURE;
            }
            if is_selected {
                feature_flags |= FLAG_SELECTED;
            }
            if instance.cloaked {
                feature_flags |= FLAG_CLOAKED;
            }
            if instance.speed_boosted {
                feature_flags |= FLAG_SPEED_BOOSTED;
            }
            uniform.feature_flags = feature_flags;

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
                    Transform::from_translation(pos).with_scale(Vec3::splat(radius)),
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
            );
        }
        return;
    }

    // If already spawned, update each hero in place with zero heap allocations
    let active_hero_idx = selection_state
        .selected_hero_index
        .unwrap_or(lab_state.selected_hero_index)
        .min(expected_heroes.saturating_sub(1));
    lab_state.selected_hero_index = active_hero_idx;

    for (_entity, mat_handle, hero_idx, mut bubble_instance, mut transform) in &mut query {
        let is_selected = hero_idx.0 == active_hero_idx;
        let Some(&arch) = heroes.get(hero_idx.0) else {
            continue;
        };

        let is_dome = matches!(arch.shape, ShapeKind::BuildingDome { .. });
        let x = match hero_idx.0 {
            0 => -3.6,
            1 => -1.2,
            2 => 1.2,
            _ => 4.2,
        };
        let radius = if is_dome { 1.35 } else { 0.85 };
        let y = if is_dome { 0.0 } else { radius };
        let pos = Vec3::new(x, y, 0.0);
        let team_id = if hero_idx.0 == 2 {
            TeamId(2)
        } else {
            TeamId(1)
        };

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

        let instance = BubbleRenderInstance {
            entity_id: EntityId(100 + hero_idx.0 as u32),
            shape: arch.shape,
            silhouette: arch.silhouette,
            unit_type: arch.unit_type_id,
            team_id,
            world_pos: [pos.x, pos.z, 0.0],
            ground_z: 0.0,
            radius,
            velocity: if is_selected {
                [lab_state.speed, 0.0]
            } else {
                [0.0, 0.0]
            },
            heading_angle_rad: if is_selected {
                lab_state.heading_angle_deg.to_radians()
            } else {
                0.0
            },
            aim_angle_rad: if is_selected {
                lab_state.aim_angle_deg.to_radians()
            } else {
                0.0
            },
            is_attacking: is_selected && lab_state.anim_state == AnimState::Attack,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: if is_selected {
                lab_state.anim_state
            } else {
                AnimState::Idle
            },
            anim_progress: (time.elapsed_secs() * 1.5).fract(),
            hp_ratio: if is_selected { lab_state.hp_ratio } else { 1.0 },
            shield_ratio: if is_selected {
                lab_state.shield_ratio
            } else {
                1.0
            },
            shield_hit_flash: if is_selected {
                lab_state.shield_hit_flash
            } else {
                0.0
            },
            hull_hit_angle_rad: if is_selected {
                lab_state.hit_angle_deg.to_radians()
            } else {
                0.0
            },
            hull_hit_flash: if is_selected {
                lab_state.hull_hit_flash
            } else {
                0.0
            },
            build_or_queue_progress: Some(if is_selected {
                lab_state.build_progress
            } else {
                1.0
            }),
            selected: is_selected,
            cloaked: is_selected && lab_state.cloaked,
            speed_boosted: is_selected && lab_state.speed_boosted,
        };

        let mut uniform = BubbleUniformGpu::from_instance(&instance, meta);
        let mut feature_flags = 0u32;
        if lab_state.opt_glare {
            feature_flags |= FLAG_GLARE;
        }
        if lab_state.opt_fresnel {
            feature_flags |= FLAG_FRESNEL;
        }
        if lab_state.opt_inner_warp {
            feature_flags |= FLAG_INNER_WARP;
        }
        if lab_state.opt_cracks {
            feature_flags |= FLAG_CRACKS;
        }
        if lab_state.opt_crt_glitch {
            feature_flags |= FLAG_CRT_GLITCH;
        }
        if lab_state.opt_facing_chevron {
            feature_flags |= FLAG_FACING_STRIPE;
        }
        if lab_state.opt_focal_lens {
            feature_flags |= FLAG_ATTACK_APERTURE;
        }
        if is_selected {
            feature_flags |= FLAG_SELECTED;
        }
        if instance.cloaked {
            feature_flags |= FLAG_CLOAKED;
        }
        if instance.speed_boosted {
            feature_flags |= FLAG_SPEED_BOOSTED;
        }
        uniform.feature_flags = feature_flags;

        if let Some(mat) = bubble_materials.get_mut(&mat_handle.0) {
            mat.uniform = uniform;
            mat.video_texture = texture;
        }

        transform.translation = pos;
        transform.scale = Vec3::splat(radius);

        // Mutate component in place without issuing commands.entity(e).insert(...)
        bubble_instance.0 = instance;

        let lab_frame = (time.elapsed_secs() * 30.0) as u64;
        let is_moving = lab_state.speed > 0.05;
        let throttled = is_moving && lab_frame % 2 == 0;

        if !is_dome && lab_state.opt_mist_wake && throttled {
            emit_mist_wake(
                &mut commands,
                &vfx_assets,
                &mut mist_materials,
                pos,
                if is_moving {
                    Vec2::new(lab_state.speed, 0.0)
                } else {
                    Vec2::ZERO
                },
                radius,
                lab_state.speed_boosted,
                true,
            );
        }
    }
}

/// Steps and synchronizes the live 2-army deterministic `sim_core` battle.
///
/// Maintains entity mappings in [`RenderEntityMap`] to achieve zero per-frame heap allocations.
pub fn sync_live_battle_system(
    mut commands: Commands<'_, '_>,
    mut lab_state: ResMut<'_, LabState>,
    time: Res<'_, Time>,
    mut combat_events: EventWriter<'_, CombatVisualEvent>,
    mut render_entities: ResMut<'_, RenderEntityMap>,
    registry: Res<'_, FlipbookRegistry>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    indicator_assets: Res<'_, IndicatorAssets>,
    vfx_assets: Res<'_, VfxSharedAssets>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>,
    mut mist_materials: ResMut<'_, Assets<MistWakeMaterial>>,
    mut query: Query<
        '_,
        '_,
        (
            Entity,
            &mut Transform,
            &mut BubbleInstance,
            &MeshMaterial3d<BubbleDomeMaterial>,
        ),
        With<LiveBattleBubble>,
    >,
) {
    // PERFORMANCE RATIONALE:
    // Why: Running world.step() directly inside Bevy's per-frame Update loop ties
    // simulation speed to monitor refresh rate (60 Hz to 240+ Hz).
    // What: We accumulate delta_secs * speed and step the 25 Hz simulation in fixed
    // 0.04s increments, capping at 5 steps per render frame to prevent spiral of death.
    // Trade-off: Perfectly deterministic 25 Hz simulation regardless of display FPS.
    const SIM_TICK_SECS: f32 = 0.04;
    const MAX_STEPS_PER_FRAME: usize = 5;

    if lab_state.current_tab != LabTab::LiveBattle {
        // Despawn battle bubbles when not on the battle tab
        for (entity, _, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        render_entities.0.clear();
        return;
    }

    if lab_state.sim_world.is_none() {
        lab_state.sim_world = Some(SimWorld::new_default_battle_sandbox());
    }

    let lab = &mut *lab_state;
    let paused = lab.battle_paused;
    let speed = lab.simulation_speed;

    let Some(ref mut world) = lab.sim_world else {
        return;
    };

    let mut latest_snapshot = None;
    let mut accumulated_combat_events = Vec::new();

    if paused || speed == 0 {
        latest_snapshot = Some(world.build_render_snapshot());
    } else {
        lab.battle_tick_accumulator_secs += time.delta_secs() * (speed as f32);

        // Clamp accumulator to avoid spiral of death after system lag spikes or window drags
        if lab.battle_tick_accumulator_secs > 0.25 {
            lab.battle_tick_accumulator_secs = 0.25;
        }

        let mut steps = 0;
        while lab.battle_tick_accumulator_secs >= SIM_TICK_SECS && steps < MAX_STEPS_PER_FRAME {
            lab.battle_tick_accumulator_secs -= SIM_TICK_SECS;
            let bundle = TickBundle {
                tick: Tick(world.current_tick),
                commands: Vec::new(),
                state_checksum: None,
            };
            let mut snap = world.step(&bundle);
            accumulated_combat_events.append(&mut snap.combat_events);
            latest_snapshot = Some(snap);
            steps += 1;
        }
    }

    let Some(snapshot) = latest_snapshot else {
        return;
    };

    lab.last_checksum_hex = format!("0x{:016X}", snapshot.checksum);

    // Forward all accumulated combat events to VFX pipeline
    for event in accumulated_combat_events {
        combat_events.send(CombatVisualEvent(event));
    }

    // Synchronize 3D Bevy entities with snapshot bubbles without per-frame allocations
    for instance in &snapshot.bubbles {
        let meta = registry.get_meta(instance.unit_type);
        let texture = registry.get_texture(instance.unit_type);
        let uniform = BubbleUniformGpu::from_instance(instance, meta);
        let is_dome = matches!(instance.shape, ShapeKind::BuildingDome { .. });
        let altitude_offset = if is_dome { 0.0 } else { instance.radius };
        let pos = Vec3::new(
            instance.world_pos[0],
            instance.world_pos[2] + altitude_offset,
            instance.world_pos[1],
        );

        let target_pos = pos;
        if let Some(&e) = render_entities.0.get(&instance.entity_id) {
            if let Ok((_, mut transform, mut bubble, mat_handle)) = query.get_mut(e) {
                if let Some(mat) = bubble_materials.get_mut(&mat_handle.0) {
                    mat.uniform = uniform;
                }
                let lerp_factor = (time.delta_secs() * 22.0).min(1.0);
                if transform.translation.distance_squared(target_pos) > 4.0
                    || transform.translation.distance_squared(target_pos) < 0.0001
                {
                    transform.translation = target_pos;
                } else {
                    transform.translation = transform.translation.lerp(target_pos, lerp_factor);
                }
                transform.scale = Vec3::splat(instance.radius);
                bubble.0 = instance.clone();
            }

            let moving = instance.velocity[0].abs() + instance.velocity[1].abs() > 0.0005;

            if moving && !is_dome {
                emit_mist_wake(
                    &mut commands,
                    &vfx_assets,
                    &mut mist_materials,
                    target_pos,
                    Vec2::new(instance.velocity[0], instance.velocity[1]),
                    instance.radius,
                    instance.speed_boosted,
                    false,
                );
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
            );
        }
    }

    // Despawn dead entities
    render_entities.0.retain(|&id, &mut entity| {
        let is_alive = snapshot.bubbles.iter().any(|b| b.entity_id == id);
        if !is_alive {
            commands.entity(entity).despawn_recursive();
        }
        is_alive
    });
}

/// Canonical alias for stepping the client simulation and battle synchronization.
pub use sync_live_battle_system as step_client_simulation_system;

/// Spawns and animates 200 colliding bubbles for the swarm stress test.
pub fn sync_swarm_stress_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    mut lab_state: ResMut<'_, LabState>,
    catalog: Res<'_, CatalogResource>,
    registry: Res<'_, FlipbookRegistry>,
    shared_meshes: Res<'_, BubbleSharedMeshes>,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
    mut query: Query<
        '_,
        '_,
        (Entity, &mut Transform, &MeshMaterial3d<BubbleDomeMaterial>),
        With<SwarmBubble>,
    >,
) {
    if lab_state.current_tab != LabTab::SwarmStressTest || !lab_state.swarm_active {
        // Despawn swarm when inactive
        for (entity, _, _) in &query {
            commands.entity(entity).despawn_recursive();
        }
        lab_state.swarm_bubble_count = 0;
        return;
    }

    let count = query.iter().count();
    lab_state.swarm_bubble_count = count;

    if count == 0 {
        let Some(mobile_arch) = catalog
            .0
            .archetypes()
            .find(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        else {
            return;
        };
        let swarm_unit_type = mobile_arch.unit_type_id;
        let swarm_silhouette = mobile_arch.silhouette;

        let meta = registry.get_meta(swarm_unit_type);
        let texture = registry.get_texture(swarm_unit_type);

        // Spawn 200 bubbles (100 Team 1 vs 100 Team 2)
        for i in 0..200u32 {
            let team_id = if i < 100 { 1 } else { 2 };
            let row = (i % 10) as f32;
            let col = ((i % 100) / 10) as f32;
            let x = if team_id == 1 {
                8.0 + col * 1.2
            } else {
                24.0 - col * 1.2
            };
            let z = 10.0 + row * 1.2;

            let instance = BubbleRenderInstance {
                entity_id: EntityId(1000 + i),
                shape: ShapeKind::UnitSphere,
                silhouette: swarm_silhouette,
                unit_type: swarm_unit_type,
                team_id: TeamId(team_id),
                world_pos: [x, z, 0.5],
                ground_z: 0.0,
                radius: 0.45,
                velocity: [if team_id == 1 { 1.5 } else { -1.5 }, 0.0],
                heading_angle_rad: if team_id == 1 {
                    0.0
                } else {
                    std::f32::consts::PI
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
            };

            let uniform = BubbleUniformGpu::from_instance(&instance, meta);

            let mat = bubble_materials.add(BubbleDomeMaterial {
                uniform,
                video_texture: texture.clone(),
                alpha_mode: AlphaMode::Blend,
            });

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
        // Animate swarm translation with boundary bouncing
        let dt = time.delta_secs();
        for (_, mut tf, mat_handle) in &mut query {
            tf.translation.x += (tf.translation.z.sin() * 0.5) * dt;
            if let Some(mat) = bubble_materials.get_mut(&mat_handle.0) {
                mat.uniform.anim_progress = (time.elapsed_secs() * 2.0).fract();
            }
        }
    }
}
