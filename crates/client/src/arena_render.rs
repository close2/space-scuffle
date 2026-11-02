//! Flat stage and light rig shared by the Visual Lab and the Swarm stress test.
//!
//! # Responsibility
//! Scenes without a loaded map need a floor and a light. Real maps, with their cliffs, ramps and
//! resources, belong to `terrain/` and are never approximated here.
//!
//! # Invariants
//! - Every entity carries [`ArenaMapMarker`], so the scene environment can hide the whole stage
//!   while a committed battle root is visible.
//! - The stage holds exactly one directional light.

use bevy::prelude::*;

/// Marker component for static stage geometry and its light.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaMapMarker;

/// Spawns the stage floor, its directional light and the ambient light.
pub fn spawn_arena_environment(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(64.0, 64.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.10, 0.14),
            metallic: 0.85,
            perceptual_roughness: 0.35,
            reflectance: 0.25,
            ..default()
        })),
        Transform::from_xyz(16.0, 0.0, 16.0),
        ArenaMapMarker,
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 14_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 32.0, 20.0).looking_at(Vec3::new(16.0, 0.0, 16.0), Vec3::Y),
        ArenaMapMarker,
        Visibility::Visible,
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.65, 0.78, 1.0),
        brightness: 350.0,
    });
}
