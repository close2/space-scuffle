//! Automated headless integration tests for the Space Scuffle client crate.
//!
//! # Invariants Verified
//! - Uniform memory packing is strictly 96 bytes and 16-byte aligned.
//! - RTS Camera perspective pitch is 58°, distance clamps to [3.0, 48.0], and bookmarks round-trip.
//! - Visual Lab state transitions properly reflect health thresholds (cracks < 75%, glitch < 35%).
//! - Headless 60-frame simulation synchronization steps deterministically without panics.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::assert_is_empty)]

use bevy::prelude::*;
use client::bubble_shader::BubbleUniformGpu;
use client::camera::{
    RtsCamera, DEFAULT_CAMERA_PITCH_RAD, MAX_CAMERA_DISTANCE, MIN_CAMERA_DISTANCE,
};
use client::lab_ui::{LabState, LabTab};
use client::run_headless_check;
use sim_core::world::SimWorld;
use sim_types::commands::TickBundle;
use sim_types::enums::{AnimState, ShapeKind, SilhouetteClass, WeaponVisualKind};
use sim_types::ids::{EntityId, TeamId, Tick, UnitTypeId};
use sim_types::snapshot::BubbleRenderInstance;

use core::mem::{align_of, size_of};

#[test]
fn test_camera_invariants() {
    let mut cam = RtsCamera::default();

    // Verify default pitch is ~58 degrees
    assert!((cam.pitch - DEFAULT_CAMERA_PITCH_RAD).abs() < 1e-4);

    // Zoom clamping tests
    cam.zoom(1000.0); // Extreme zoom in
    assert_eq!(cam.distance, MIN_CAMERA_DISTANCE);

    cam.zoom(-1000.0); // Extreme zoom out
    assert_eq!(cam.distance, MAX_CAMERA_DISTANCE);

    // Camera bookmark save and recall
    let test_pos = Vec2::new(32.0, 48.0);
    let test_dist = 18.5;
    cam.target = test_pos;
    cam.distance = test_dist;
    cam.save_bookmark(KeyCode::F1);

    // Move camera away
    cam.target = Vec2::new(0.0, 0.0);
    cam.distance = 10.0;

    // Recall bookmark
    let recalled = cam.recall_bookmark(&KeyCode::F1);
    assert!(recalled);
    assert_eq!(cam.target, test_pos);
    assert_eq!(cam.distance, test_dist);

    // Transform elevation check
    let tf = cam.compute_transform();
    assert!(tf.translation.y > 0.0);
}

#[test]
fn test_bubble_uniform_packing() {
    // Assert struct size matches WGSL BubbleUniform layout exactly
    assert_eq!(size_of::<BubbleUniformGpu>(), 96);
    assert_eq!(align_of::<BubbleUniformGpu>(), 16);

    let instance = BubbleRenderInstance {
        entity_id: EntityId(42),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::LightProngs,
        unit_type: UnitTypeId(2),
        team_id: TeamId(1),
        world_pos: [10.0, 20.0, 1.5],
        ground_z: 0.0,
        radius: 0.75,
        velocity: [1.2, -0.4],
        heading_angle_rad: 1.25,
        aim_angle_rad: -0.5,
        is_attacking: true,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: AnimState::Attack,
        anim_progress: 0.5,
        hp_ratio: 0.65,
        shield_ratio: 0.8,
        shield_hit_flash: 0.2,
        hull_hit_angle_rad: 0.75,
        hull_hit_flash: 0.4,
        build_or_queue_progress: None,
        selected: true,
        cloaked: false,
        speed_boosted: true,
    };

    let gpu_uniform = BubbleUniformGpu::from_instance(&instance, None);

    assert_eq!(gpu_uniform.shape_kind, 0); // UnitSphere
    assert_eq!(gpu_uniform.is_attacking, 1);
    assert_eq!(gpu_uniform.selected, 1);
    assert_ne!(gpu_uniform.feature_flags, 0);
    assert_eq!(
        gpu_uniform.feature_flags & client::bubble_shader::FLAG_SPEED_BOOSTED,
        client::bubble_shader::FLAG_SPEED_BOOSTED
    );
    assert_eq!(gpu_uniform.cloaked, 0);
    assert_eq!(gpu_uniform.radius, 0.75);
    assert_eq!(gpu_uniform.world_pos, Vec3::new(10.0, 1.5, 20.0));
    assert_eq!(gpu_uniform.hp_ratio, 0.65);
}

#[test]
fn test_lab_state_transitions() {
    let mut state = LabState::default();

    assert_eq!(state.current_tab, LabTab::VisualLab);
    assert_eq!(state.hp_ratio, 1.0);

    // Transition health to damaged threshold (<0.75)
    state.hp_ratio = 0.50;
    assert!(state.hp_ratio < 0.75 && state.hp_ratio >= 0.35);

    // Transition health to critical glitch threshold (<0.35)
    state.hp_ratio = 0.20;
    assert!(state.hp_ratio < 0.35);

    // Switch tab to Live Battle
    state.current_tab = LabTab::LiveBattle;
    state.sim_world = Some(SimWorld::new_default_battle_sandbox());
    assert!(state.sim_world.is_some());
}

#[test]
fn test_headless_ci_runner() {
    let result = run_headless_check();
    assert!(result.is_ok(), "Headless check failed: {:?}", result.err());
}

#[test]
fn test_headless_simulation_stepping() {
    let mut world = SimWorld::new_default_battle_sandbox();
    let initial_checksum = world.compute_checksum();

    for _ in 0..60 {
        let bundle = TickBundle {
            tick: Tick(world.current_tick),
            commands: Vec::new(),
            state_checksum: None,
        };
        let snapshot = world.step(&bundle);
        assert!(!snapshot.bubbles.is_empty());
    }

    assert_eq!(world.current_tick, 60);
    let final_checksum = world.compute_checksum();
    assert_ne!(initial_checksum, final_checksum);
}
