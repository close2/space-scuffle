//! Interactive playtest mode inside the map editor.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.6 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides the in-editor playtesting environment. It builds a deterministic
//! [`SimWorld`] from the active [`MapDocument`] and the authored rule catalogs, then hands it to
//! the same production pipeline the game client uses: [`BattleSession`] owns the command queue,
//! tick clock and published snapshots, and [`advance_battle_ticks`] is the only tick driver.
//!
//! [`advance_battle_ticks`]: crate::battle_pipeline::driver::advance_battle_ticks
//!
//! # Invariants Maintained
//! 1. **Deterministic Seed:** Always builds the playtest world using named constant [`PLAYTEST_SEED`] (`0x5CA1_AB1E`).
//! 2. **Transactional Entry:** World, session and media are prepared and checked, and unsaved work is
//!    autosaved, before any live state changes. A failed entry leaves the editor exactly as it was.
//! 3. **Non-Destructive Playtest:** Exiting playtest via `Escape` restores the editor state, document,
//!    history, and camera completely unmodified.
//! 4. **Clean Teardown:** Completely despawns all combat bubbles and visual FX entities upon exit.
//! 5. **Autosave Is Not a Save:** The autosave copy never replaces the user's file and never clears
//!    [`EditorDocument::dirty`].

use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim_core::catalog::RuntimeCatalogs;
use sim_core::scenario::{
    build_world_with_catalogs, PlayerSetup, ScenarioError, StartLoadoutCatalog,
};
use sim_core::world::SimWorld;
use sim_map::map_format::{MapDocument, MapFormatError};
use sim_map::validation::validate_map;
use sim_types::commands::{PlayerCommand, UnitOrder};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2};

use crate::battle_pipeline::driver::PipelineError;
use crate::battle_pipeline::session::{
    BattleSession, CommittedBattleMetadata, ParticipantMetadata,
};
use crate::battle_selection::BattleSelection;
use crate::bubble_shader::{BubbleInstance, FlipbookRegistry};
use crate::editor::document::EditorDocument;
use crate::editor::los_preview::StartLoadoutCatalogResource;
use crate::lab_ui::{LabState, LabTab};
use crate::media::{ClientMediaError, ClientMediaStatus};
use crate::player_presentation::{PlayerPalette, PresentationError};
use crate::scene_controller::{LiveBattleBubble, RenderEntityMap};
use crate::selection::ray_sphere_intersect;
use crate::terrain::{pick_terrain, TerrainState};
use crate::vfx::CombatFxMaterial;

/// Authoritative deterministic random seed for editor playtests.
pub const PLAYTEST_SEED: u64 = 0x5CA1_AB1E;

/// Suffix appended to a map's file name to form its autosave path.
///
/// The file browser lists only `.scmap.ron` names, so autosaves never appear as openable maps.
pub const AUTOSAVE_SUFFIX: &str = ".autosave";

/// File name stem used to autosave a map that has never been saved.
const UNTITLED_MAP_FILE: &str = "untitled.scmap.ron";

/// High-level application mode for the map editor.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorPhase {
    /// Interactive editing mode with full toolbox and inspector panels.
    #[default]
    Editing,
    /// Live battle simulation playtesting.
    Playtesting,
}

/// Newtype resource wrapper around the external unit, structure and upgrade catalogs.
#[derive(Resource, Debug, Clone, Deref)]
pub struct EditorRulesResource(pub RuntimeCatalogs);

/// ECS Resource mirroring playtest status for the HUD.
#[derive(Resource, Debug, Default)]
pub struct PlaytestState {
    /// Tick of the latest published simulation snapshot.
    pub current_tick: u64,
    /// Checksum of the latest published simulation snapshot.
    pub last_checksum: u64,
}

/// Typed reason a playtest could not start; the editor stays in [`EditorPhase::Editing`].
#[derive(Debug, thiserror::Error)]
pub enum PlaytestEntryError {
    /// The map has validation errors.
    #[error("map has {count} validation error(s); first: {first}")]
    InvalidMap {
        /// Number of validation errors.
        count: usize,
        /// Display text of the first error.
        first: String,
    },
    /// A start location names a player slot that has no loadout selector.
    #[error("start location uses unsupported player slot {0}")]
    PlayerSlot(u8),
    /// A selected loadout is absent from the catalog.
    #[error("start loadout {0} is absent from the catalog")]
    UnknownLoadout(String),
    /// Deterministic world construction failed.
    #[error(transparent)]
    Scenario(#[from] ScenarioError),
    /// A participant has no authored ownership color.
    #[error(transparent)]
    Presentation(#[from] PresentationError),
    /// The battle session could not be prepared.
    #[error(transparent)]
    Pipeline(#[from] PipelineError),
    /// A starting unit has no admitted media, so it could not be drawn.
    #[error(transparent)]
    Media(#[from] ClientMediaError),
    /// Unsaved work could not be protected, so the playtest was not started.
    #[error("cannot autosave before playtest: {0}")]
    Autosave(#[from] MapFormatError),
}

/// Lists one [`PlayerSetup`] per start location using its actual player slot.
///
/// `player_loadouts` holds the slug selected in the toolbox for each slot.
///
/// # Errors
/// Returns [`PlaytestEntryError::PlayerSlot`] if a slot has no selector.
pub fn playtest_players(
    doc: &MapDocument,
    player_loadouts: &[String],
) -> Result<Vec<PlayerSetup>, PlaytestEntryError> {
    doc.start_locations
        .iter()
        .map(|start| {
            let loadout = player_loadouts
                .get(usize::from(start.player_slot))
                .ok_or(PlaytestEntryError::PlayerSlot(start.player_slot))?;
            Ok(PlayerSetup {
                player_slot: start.player_slot,
                loadout_slug: loadout.clone(),
            })
        })
        .collect()
}

/// Attempts to build a playtest simulation world from the current document.
///
/// The world uses `rules`, the same authored catalogs as the game client, never the embedded
/// starter roster, so every simulated unit type has a matching media registry entry.
///
/// # Errors
/// Returns [`ScenarioError`] if map validation fails or scenario construction encounters errors.
pub fn initialize_playtest_world(
    doc: &EditorDocument,
    loadouts: &StartLoadoutCatalog,
    rules: &RuntimeCatalogs,
    players: &[PlayerSetup],
) -> Result<SimWorld, ScenarioError> {
    let report = validate_map(&doc.doc);
    if !report.errors.is_empty() {
        return Err(ScenarioError::InvalidMap(report.errors));
    }

    build_world_with_catalogs(
        &doc.doc,
        loadouts,
        players,
        PLAYTEST_SEED,
        rules.units.clone(),
        rules.upgrades.clone(),
    )
}

/// Builds the participant labels and colors a live session validates ownership against.
///
/// # Errors
/// Returns an error if a loadout or ownership color is absent.
pub fn playtest_metadata(
    doc: &MapDocument,
    loadouts: &StartLoadoutCatalog,
    palette: &PlayerPalette,
    players: &[PlayerSetup],
) -> Result<CommittedBattleMetadata, PlaytestEntryError> {
    let mut participants = Vec::with_capacity(players.len());
    for player in players {
        let loadout = loadouts
            .get(&player.loadout_slug)
            .ok_or_else(|| PlaytestEntryError::UnknownLoadout(player.loadout_slug.clone()))?;
        let owner = PlayerId(player.player_slot);
        participants.push(ParticipantMetadata {
            owner,
            label: format!("Player {} · {}", player.player_slot, loadout.display_name),
            color: palette.resolve(owner)?.ui,
        });
    }
    participants.sort_by_key(|participant| participant.owner);
    Ok(CommittedBattleMetadata {
        map_title: doc.metadata.title.clone(),
        players: participants,
    })
}

/// Returns where `doc` is autosaved: beside its file, or in `maps_dir` if it was never saved.
#[must_use]
pub fn autosave_path(doc: &EditorDocument, maps_dir: &Path) -> PathBuf {
    let mut path = doc
        .path
        .clone()
        .unwrap_or_else(|| maps_dir.join(UNTITLED_MAP_FILE))
        .into_os_string();
    path.push(AUTOSAVE_SUFFIX);
    PathBuf::from(path)
}

/// Everything needed to validate, prepare and commit a playtest.
#[derive(SystemParam)]
pub struct PlaytestEntry<'w> {
    /// Owner of the live simulation world.
    pub lab: ResMut<'w, LabState>,
    /// Live battle pipeline replaced by a successful entry.
    pub session: ResMut<'w, BattleSession>,
    /// Status shown on the HUD.
    pub state: ResMut<'w, PlaytestState>,
    /// Transition handle to switch editor phases.
    pub phase: ResMut<'w, NextState<EditorPhase>>,
    /// External unit, structure and upgrade rules.
    pub rules: Res<'w, EditorRulesResource>,
    /// Starting army compositions.
    pub loadouts: Res<'w, StartLoadoutCatalogResource>,
    /// Ownership colors.
    pub palette: Res<'w, PlayerPalette>,
    /// Loaded unit media handles.
    pub registry: Res<'w, FlipbookRegistry>,
    /// Admission evidence for the loaded unit media.
    pub media: Res<'w, ClientMediaStatus>,
    /// Local selection, which names units of the replaced session.
    pub selection: ResMut<'w, BattleSelection>,
}

impl PlaytestEntry<'_> {
    /// Validates `doc`, prepares a battle, autosaves unsaved work and commits the transition.
    ///
    /// Nothing live changes until every fallible step has succeeded, so a failed entry leaves
    /// the current phase, session and document intact. Returns the autosave path, if one was
    /// written.
    ///
    /// # Errors
    /// Returns [`PlaytestEntryError`] if the map is invalid, the battle cannot be prepared, a
    /// starting unit has no media, or unsaved work cannot be autosaved.
    pub fn enter(
        &mut self,
        doc: &mut EditorDocument,
        player_loadouts: &[String],
        maps_dir: &Path,
    ) -> Result<Option<PathBuf>, PlaytestEntryError> {
        doc.revalidate();
        if let Some(first) = doc.validation.errors.first() {
            return Err(PlaytestEntryError::InvalidMap {
                count: doc.validation.errors.len(),
                first: first.to_string(),
            });
        }

        let players = playtest_players(&doc.doc, player_loadouts)?;
        let metadata = playtest_metadata(&doc.doc, &self.loadouts.0, &self.palette, &players)?;
        let mut world = initialize_playtest_world(doc, &self.loadouts.0, &self.rules.0, &players)?;
        let generation = self.session.next_generation()?;
        let session = BattleSession::prepare(generation, &mut world, metadata)?;

        // Refusing here keeps a missing file a visible banner. The tick driver would instead
        // record it as a failure that outlives the session and blocks every later playtest.
        for bubble in &session.frames.current().bubbles {
            self.media
                .require(&self.registry, bubble.unit_type, bubble.anim_state)?;
        }

        let autosaved = if doc.dirty {
            let path = autosave_path(doc, maps_dir);
            doc.doc.encode().save_atomic(&path)?;
            Some(path)
        } else {
            None
        };

        *self.lab = LabState {
            current_tab: LabTab::LiveBattle,
            ..default()
        };
        *self.session = session.with_world(world);
        self.selection.clear_all();
        self.state.current_tick = 0;
        self.state.last_checksum = 0;
        self.phase.set(EditorPhase::Playtesting);
        Ok(autosaved)
    }
}

/// System returning to editing when `Escape` is pressed during playtest.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn playtest_exit_system(
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    mut next_phase: ResMut<'_, NextState<EditorPhase>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next_phase.set(EditorPhase::Editing);
    }
}

/// System mirroring the latest published snapshot's tick and checksum into [`PlaytestState`].
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn sync_playtest_status_system(
    session: Res<'_, BattleSession>,
    mut playtest_state: ResMut<'_, PlaytestState>,
) {
    let snapshot = session.frames.current();
    let tick = u64::from(snapshot.tick.0);
    if playtest_state.current_tick != tick || playtest_state.last_checksum != snapshot.checksum {
        playtest_state.current_tick = tick;
        playtest_state.last_checksum = snapshot.checksum;
    }
}

/// System drawing the playtest status bar, order feedback and any battle failure.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn playtest_hud_system(
    mut contexts: EguiContexts<'_, '_>,
    playtest_state: Res<'_, PlaytestState>,
    lab: Res<'_, LabState>,
    session: Res<'_, BattleSession>,
    mut next_phase: ResMut<'_, NextState<EditorPhase>>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    egui::TopBottomPanel::top("playtest_top_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "PLAYTESTING (25 Hz)");
            ui.separator();
            ui.label(format!("Tick: {}", playtest_state.current_tick));
            ui.separator();
            ui.label(format!("Checksum: 0x{:016X}", playtest_state.last_checksum));
            ui.separator();
            ui.label(
                "Controls: Left-click select (Shift add), Right-click move, Ctrl+Right-click attack-move",
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Return to Editor (Esc)").clicked() {
                    next_phase.set(EditorPhase::Editing);
                }
            });
        });
        let order_status = session.commands.status();
        if !order_status.is_empty() {
            ui.label(order_status);
        }
        if let Some(failure) = &lab.battle_failure {
            ui.colored_label(egui::Color32::from_rgb(255, 80, 80), failure);
        }
    });
}

/// System handling unit selection and move/attack orders during playtest.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn playtest_input_system(
    mouse_buttons: Res<'_, ButtonInput<MouseButton>>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    windows: Query<'_, '_, &Window>,
    camera_query: Query<'_, '_, (&Camera, &GlobalTransform), With<crate::camera::RtsCamera>>,
    mut contexts: EguiContexts<'_, '_>,
    terrain_state: Option<Res<'_, TerrainState>>,
    mut selection: ResMut<'_, BattleSelection>,
    mut session: ResMut<'_, BattleSession>,
    bubble_query: Query<'_, '_, (&GlobalTransform, &BubbleInstance), With<LiveBattleBubble>>,
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

        for (transform, instance) in &bubble_query {
            if instance.0.owner != PlayerId(0) {
                continue;
            }
            let center = transform.translation();
            let radius = instance.0.radius;

            if let Some(t) = ray_sphere_intersect(ray_origin, ray_dir, center, radius) {
                if closest_hit.as_ref().is_none_or(|(_, min_t)| t < *min_t) {
                    closest_hit = Some((instance.0.entity_id, t));
                }
            }
        }

        if let Some((hit_id, _)) = closest_hit {
            if shift {
                selection.toggle(hit_id);
            } else {
                selection.select_only(hit_id);
            }
        } else if !shift {
            selection.clear();
        }
    }

    // 2. Right Click: Issue Move or Attack-Move Order to Selected Units
    if mouse_buttons.just_pressed(MouseButton::Right) && !selection.ids().is_empty() {
        let Some(terrain) = &terrain_state else {
            return;
        };
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
            let command = PlayerCommand {
                player_id: PlayerId(0),
                entities: selection.ids().to_vec(),
                order,
                queue: shift,
            };
            let (world, queue) = session.world_and_commands();
            let Some(world) = world else {
                return;
            };
            if let Err(error) = queue.submit(world, command) {
                queue.reject(&error);
            }
        }
    }
}

/// System that cleans up playtest entities, bubbles, and visual effects upon exiting to editing.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn teardown_playtest_system(
    mut commands: Commands<'_, '_>,
    mut render_entities: ResMut<'_, RenderEntityMap>,
    mut selection: ResMut<'_, BattleSelection>,
    mut lab: ResMut<'_, LabState>,
    mut session: ResMut<'_, BattleSession>,
    bubble_query: Query<'_, '_, Entity, With<LiveBattleBubble>>,
    vfx_fx_query: Query<'_, '_, Entity, With<MeshMaterial3d<CombatFxMaterial>>>,
) {
    for entity in &bubble_query {
        commands.entity(entity).despawn_recursive();
    }
    for entity in &vfx_fx_query {
        commands.entity(entity).despawn_recursive();
    }

    render_entities.0.clear();
    session.suspend();
    session.release_world();
    *lab = LabState {
        current_tab: LabTab::LiveBattle,
        ..default()
    };
    selection.clear_all();
}
