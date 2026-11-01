//! Interactive playtest mode inside the map editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.6 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the in-editor playtesting environment. It initializes a deterministic
//! [`SimWorld`](sim_core::world::SimWorld) from the active [`MapDocument`](sim_map::MapDocument),
//! steps simulation at fixed 25 Hz, drives player 0 unit commands, and cleanly tears down back to editing.
//!
//! # Invariants Maintained
//! 1. **Deterministic Seed:** Always builds the playtest world using named constant [`PLAYTEST_SEED`] (`0x5CA1_AB1E`).
//! 2. **Fixed 25 Hz Stepping:** Steps simulation via a fixed 0.04s accumulator, identical to production battle timing.
//! 3. **Non-Destructive Playtest:** Exiting playtest via `Escape` restores the editor state, document,
//!    history, and camera completely unmodified.
//! 4. **Clean Teardown:** Completely despawns all combat bubbles and visual FX entities upon exit.

use bevy::prelude::*;
use bevy_egui::EguiContexts;
use sim_core::scenario::{build_world, PlayerSetup, ScenarioError, StartLoadoutCatalog};
use sim_core::world::SimWorld;
use sim_map::validation::validate_map;
use sim_types::commands::{PlayerCommand, TickBundle, UnitOrder};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, Tick};

use crate::bubble_shader::{BubbleDomeMaterial, BubbleInstance};
use crate::editor::document::EditorDocument;
use crate::lab_ui::LabState;
use crate::scene_controller::{LiveBattleBubble, RenderEntityMap};
use crate::selection::ray_sphere_intersect;
use crate::terrain::{pick_terrain, TerrainState};
use crate::vfx::{CombatFxMaterial, CombatVisualEvent, MistWakeMaterial};

/// Authoritative deterministic random seed for editor playtests.
pub const PLAYTEST_SEED: u64 = 0x5CA1_AB1E;

/// Fixed duration in seconds of a single simulation tick (25 Hz).
pub const SIM_TICK_SECS: f32 = 0.04;

/// Maximum number of simulation ticks executed per render frame to prevent spiral of death.
pub const MAX_STEPS_PER_FRAME: usize = 5;

/// High-level application mode for the map editor.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorPhase {
    /// Interactive editing mode with full toolbox and inspector panels.
    #[default]
    Editing,
    /// Live battle simulation playtesting.
    Playtesting,
}

/// ECS Resource managing playtest execution and pending orders.
#[derive(Resource, Debug, Default)]
pub struct PlaytestState {
    /// Pending player commands waiting to be consumed on the next simulation tick.
    pub pending_commands: Vec<PlayerCommand>,
    /// Accumulated time in seconds awaiting fixed-step consumption.
    pub accumulator_secs: f32,
    /// Currently selected player unit IDs.
    pub selected_unit_ids: Vec<EntityId>,
    /// Last simulation tick index stepped.
    pub current_tick: u64,
    /// Checksum of the latest simulation state.
    pub last_checksum: u64,
    /// Error banner if entering playtest failed.
    pub error_banner: Option<String>,
}

/// Attempts to build a playtest simulation world from the current document.
///
/// # Errors
/// Returns [`ScenarioError`] if map validation fails or scenario construction encounters errors.
pub fn initialize_playtest_world(
    doc: &EditorDocument,
    loadouts: &StartLoadoutCatalog,
    players: &[PlayerSetup],
) -> Result<SimWorld, ScenarioError> {
    let report = validate_map(&doc.doc);
    if !report.errors.is_empty() {
        return Err(ScenarioError::InvalidMap(report.errors));
    }

    build_world(&doc.doc, loadouts, players, PLAYTEST_SEED)
}

/// System stepping the deterministic simulation at 25 Hz during playtest.
pub fn playtest_step_system(
    time: Res<'_, Time>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    mut next_phase: ResMut<'_, NextState<EditorPhase>>,
    mut playtest_state: ResMut<'_, PlaytestState>,
    mut lab_state: ResMut<'_, LabState>,
    mut combat_events: EventWriter<'_, CombatVisualEvent>,
) {
    // 1. Check for Escape key to exit playtest
    if keyboard.just_pressed(KeyCode::Escape) {
        next_phase.set(EditorPhase::Editing);
        return;
    }

    let Some(ref mut world) = lab_state.sim_world else {
        return;
    };

    // 2. Accumulate delta time
    playtest_state.accumulator_secs += time.delta_secs();
    if playtest_state.accumulator_secs > 0.25 {
        playtest_state.accumulator_secs = 0.25;
    }

    // 3. Step in fixed increments
    let mut steps = 0;
    while playtest_state.accumulator_secs >= SIM_TICK_SECS && steps < MAX_STEPS_PER_FRAME {
        playtest_state.accumulator_secs -= SIM_TICK_SECS;
        steps += 1;

        let tick = Tick(world.current_tick);
        let commands = std::mem::take(&mut playtest_state.pending_commands);
        let bundle = TickBundle {
            tick,
            commands,
            state_checksum: None,
        };

        let snapshot = world.step(&bundle);
        playtest_state.current_tick = u64::from(snapshot.tick.0);
        playtest_state.last_checksum = snapshot.checksum;

        for ev in snapshot.combat_events {
            combat_events.send(CombatVisualEvent(ev));
        }
    }
}

/// System handling unit selection and move/attack orders during playtest.
pub fn playtest_input_system(
    mouse_buttons: Res<'_, ButtonInput<MouseButton>>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    windows: Query<'_, '_, &Window>,
    camera_query: Query<'_, '_, (&Camera, &GlobalTransform), With<crate::camera::RtsCamera>>,
    mut contexts: EguiContexts<'_, '_>,
    terrain_state: Option<Res<'_, TerrainState>>,
    mut playtest_state: ResMut<'_, PlaytestState>,
    mut bubble_query: Query<
        '_,
        '_,
        (
            Entity,
            &GlobalTransform,
            &mut BubbleInstance,
            &MeshMaterial3d<BubbleDomeMaterial>,
        ),
        With<LiveBattleBubble>,
    >,
    mut bubble_materials: ResMut<'_, Assets<BubbleDomeMaterial>>,
) {
    if let Some(ctx) = contexts.try_ctx_mut() {
        if ctx.wants_pointer_input() {
            return;
        }
    }

    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let ray_origin = ray.origin;
    let ray_dir = *ray.direction;
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);

    // 1. Left Click: Select Player 0 Unit Bubble
    if mouse_buttons.just_pressed(MouseButton::Left) {
        let mut closest_hit: Option<(EntityId, f32)> = None;

        for (_ent, transform, instance, _) in &bubble_query {
            if instance.0.owner != PlayerId(0) {
                continue;
            }
            let center = transform.translation();
            let radius = instance.0.radius;

            if let Some(t) = ray_sphere_intersect(ray_origin, ray_dir, center, radius) {
                if closest_hit.as_ref().map_or(true, |(_, min_t)| t < *min_t) {
                    closest_hit = Some((instance.0.entity_id, t));
                }
            }
        }

        if let Some((hit_id, _)) = closest_hit {
            if shift {
                if let Some(pos) = playtest_state
                    .selected_unit_ids
                    .iter()
                    .position(|&id| id == hit_id)
                {
                    playtest_state.selected_unit_ids.swap_remove(pos);
                } else {
                    playtest_state.selected_unit_ids.push(hit_id);
                }
            } else {
                playtest_state.selected_unit_ids.clear();
                playtest_state.selected_unit_ids.push(hit_id);
            }
        } else if !shift {
            playtest_state.selected_unit_ids.clear();
        }

        // Synchronize visual selected flag
        for (_, _, mut instance, mat_handle) in &mut bubble_query {
            let is_selected = playtest_state
                .selected_unit_ids
                .contains(&instance.0.entity_id);
            instance.0.selected = is_selected;
            if let Some(mat) = bubble_materials.get_mut(&mat_handle.0) {
                mat.uniform.selected = u32::from(is_selected);
            }
        }
    }

    // 2. Right Click: Issue Move or Attack-Move Order to Selected Units
    if mouse_buttons.just_pressed(MouseButton::Right)
        && !playtest_state.selected_unit_ids.is_empty()
    {
        if let Some(ref terrain) = terrain_state {
            if let Some(hit) = pick_terrain(&terrain.map, ray_origin, ray_dir) {
                let target = SimVec2::new(
                    SimScalar::from_num(hit.point.x),
                    SimScalar::from_num(hit.point.z),
                );

                let order = if ctrl {
                    UnitOrder::AttackMove { target }
                } else {
                    UnitOrder::Move { target }
                };

                let entities = playtest_state.selected_unit_ids.clone();
                playtest_state.pending_commands.push(PlayerCommand {
                    player_id: PlayerId(0),
                    entities,
                    order,
                    queue: shift,
                });
            }
        }
    }
}

/// System that cleans up playtest entities, bubbles, and visual effects upon exiting to editing.
pub fn teardown_playtest_system(
    mut commands: Commands<'_, '_>,
    mut render_entities: ResMut<'_, RenderEntityMap>,
    mut playtest_state: ResMut<'_, PlaytestState>,
    bubble_query: Query<'_, '_, Entity, With<LiveBattleBubble>>,
    vfx_fx_query: Query<'_, '_, Entity, With<MeshMaterial3d<CombatFxMaterial>>>,
    vfx_wake_query: Query<'_, '_, Entity, With<MeshMaterial3d<MistWakeMaterial>>>,
) {
    for entity in &bubble_query {
        commands.entity(entity).despawn_recursive();
    }
    for entity in &vfx_fx_query {
        commands.entity(entity).despawn_recursive();
    }
    for entity in &vfx_wake_query {
        commands.entity(entity).despawn_recursive();
    }

    render_entities.0.clear();
    commands.remove_resource::<LabState>();
    playtest_state.selected_unit_ids.clear();
    playtest_state.pending_commands.clear();
    playtest_state.accumulator_secs = 0.0;
}
