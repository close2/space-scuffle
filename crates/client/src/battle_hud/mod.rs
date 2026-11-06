//! The heads-up display of a person playing one side of a battle.
//!
//! # Responsibility
//! Replaces the developer inspector under
//! [`BattleProfile::Player`](crate::battle_presentation::BattleProfile::Player): resources, supply
//! and clock along the top, selection along the bottom, a message feed above it, and a result
//! window when the match ends. This module owns the plugin, the system order and the rectangles
//! the panels occupy.
//!
//! # How it fits
//! ```text
//! CollectIntent        battle_hud_system, battle_result_system, then lab_ui_system computes the
//!                      world rectangle
//!                      apply_hud_intent, before collect_pointer_intent
//!                      collect_card_keys, after collect_key_intent
//! PreparePresentation  sync_live_battle_system, refresh_hud_model, refresh_battle_fog,
//!                      apply_battle_fog, refresh_hud_focus, collect_battle_feed,
//!                      refresh_command_card, refresh_queue_panel, refresh_result_screen
//! Present              update_placement_preview
//! ```
//! The bars are egui panels drawn before [`lab_ui_system`](crate::lab_ui_system) measures what is
//! left, so that rectangle is both the camera viewport and the hit test
//! [`sample_input_ownership`](crate::input_policy::sample_input_ownership) applies: a press on a
//! panel cannot reach the world, with no second mechanism to keep in step. The result window
//! floats and reserves nothing: that same system refuses the world any pointer over it.
//!
//! This module depends on `battle_input` and `battle_pipeline`, never the reverse. The HUD reads
//! [`HudModel`](model::HudModel), which is filled from a viewer-scoped
//! [`PlayerView`](sim_types::PlayerView), and never reads the world while drawing.
//!
//! # Invariants
//! - Every system here runs only under the `Player` profile, so `Sandbox` is untouched.
//! - Nothing here names a unit or structure: all text comes from catalog data.
//! - [`HudGeometry`] holds only rectangles drawn on the current frame.

pub mod actions;
pub mod card;
pub mod catalog;
pub mod feed;
pub mod fog;
pub mod model;
pub mod panels;
pub mod placement;
pub mod queue;
pub mod result;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::battle_input::{collect_key_intent, collect_pointer_intent};
use crate::battle_pipeline::fog::BattleFog;
use crate::battle_pipeline::BattleSet;
use crate::battle_presentation::BattleProfile;
use crate::input_bindings::COMMAND_CARD_SLOTS;

/// Visible rectangles of HUD surfaces and controls, in egui points, recorded each frame.
///
/// Tests aim real pointer events with these, and they are the evidence of what was drawn. A field
/// is `None` when that surface was not drawn on the current frame.
#[derive(Resource, Debug, Default)]
pub struct HudGeometry {
    /// The resource bar along the top of the window.
    pub top: Option<egui::Rect>,
    /// The command panel along the bottom of the window.
    pub bottom: Option<egui::Rect>,
    /// The failure banner above the resource bar.
    pub failure: Option<egui::Rect>,
    /// The left column of the command panel.
    pub selection_column: Option<egui::Rect>,
    /// The middle column of the command panel.
    pub queue_column: Option<egui::Rect>,
    /// The right column of the command panel.
    pub card_column: Option<egui::Rect>,
    /// The message feed floating above the command panel.
    pub feed: Option<egui::Rect>,
    /// The pause button of the resource bar.
    pub pause: Option<egui::Rect>,
    /// The button lowering the simulation speed.
    pub speed_down: Option<egui::Rect>,
    /// The button raising the simulation speed.
    pub speed_up: Option<egui::Rect>,
    /// The button resetting a failed battle.
    pub reset: Option<egui::Rect>,
    /// The filled command card positions, in slot order.
    pub card: [Option<egui::Rect>; COMMAND_CARD_SLOTS],
    /// Stop and Hold.
    pub card_fixed: [Option<egui::Rect>; card::FIXED_SLOTS],
    /// The queue rows, front of the queue first.
    pub queue: [Option<egui::Rect>; queue::QUEUE_ROWS],
    /// The button cancelling the selected foundation.
    pub queue_cancel: Option<egui::Rect>,
    /// The note saying why the site under the pointer is refused.
    pub site_note: Option<egui::Rect>,
    /// The window showing how the match ended.
    pub result: Option<egui::Rect>,
    /// The button of the result window that starts a new match.
    pub play_again: Option<egui::Rect>,
    /// The button of the result window that leaves the game.
    pub exit: Option<egui::Rect>,
}

/// Which optional HUD surfaces the `Player` profile draws.
///
/// The game always draws everything ([`HudLayout::default`]). A non-interactive presentation of a
/// battle, such as the CI showcase video of `client_diagnostics`, may leave out the command panel,
/// which then reserves no room: the world viewport extends to the bottom of the window. Nothing
/// else changes; the selection, the card and their keys keep working, and a run that draws the
/// panel again gets it back on the next frame.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudLayout {
    /// Whether the command panel (selection, queue and card) is drawn along the bottom.
    pub command_panel: bool,
}

impl Default for HudLayout {
    fn default() -> Self {
        Self {
            command_panel: true,
        }
    }
}

/// Registers the HUD. Added by [`ClientAppPlugin`](crate::ClientAppPlugin) only, because the map
/// editor and the headless check present no player.
pub struct BattleHudPlugin;

impl Plugin for BattleHudPlugin {
    fn build(&self, app: &mut App) {
        // Readers take the fog as an `Option`, and its absence is what keeps `Sandbox` unfiltered.
        if app.world().get_resource::<BattleProfile>() == Some(&BattleProfile::Player) {
            app.init_resource::<BattleFog>();
        }
        app.init_resource::<HudGeometry>()
            .init_resource::<HudLayout>()
            .init_resource::<model::HudModel>()
            .init_resource::<feed::BattleFeed>()
            .init_resource::<card::CommandCard>()
            .init_resource::<queue::QueuePanel>()
            .init_resource::<actions::HudIntent>()
            .init_resource::<placement::PlacementPreview>()
            .add_systems(
                Startup,
                placement::setup_placement_ghost.run_if(resource_equals(BattleProfile::Player)),
            )
            .init_resource::<result::ResultScreen>()
            .add_systems(
                Update,
                (
                    panels::battle_hud_system
                        .before(crate::lab_ui_system)
                        .in_set(BattleSet::CollectIntent),
                    actions::apply_hud_intent
                        .after(panels::battle_hud_system)
                        .before(collect_pointer_intent)
                        .in_set(BattleSet::CollectIntent),
                    actions::collect_card_keys
                        .after(collect_key_intent)
                        .in_set(BattleSet::CollectIntent),
                    (
                        model::refresh_hud_model,
                        fog::refresh_battle_fog,
                        fog::apply_battle_fog,
                        model::refresh_hud_focus,
                        feed::collect_battle_feed,
                        card::refresh_command_card,
                        queue::refresh_queue_panel,
                    )
                        .chain()
                        .after(crate::sync_live_battle_system)
                        .in_set(BattleSet::PreparePresentation),
                    placement::update_placement_preview.in_set(BattleSet::Present),
                )
                    .run_if(resource_equals(BattleProfile::Player)),
            )
            .add_systems(
                Update,
                (
                    result::battle_result_system
                        .after(panels::battle_hud_system)
                        .before(crate::lab_ui_system)
                        .in_set(BattleSet::CollectIntent),
                    result::refresh_result_screen
                        .after(model::refresh_hud_model)
                        .in_set(BattleSet::PreparePresentation),
                )
                    .run_if(resource_equals(BattleProfile::Player)),
            );
    }
}
