//! The heads-up display of a person playing one side of a battle.
//!
//! # Responsibility
//! Replaces the developer inspector under
//! [`BattleProfile::Player`](crate::battle_presentation::BattleProfile::Player): resources, supply
//! and clock along the top, selection along the bottom, and a message feed above it. This module
//! owns the plugin, the system order and the rectangles the panels occupy.
//!
//! # How it fits
//! ```text
//! CollectIntent        battle_hud_system, then lab_ui_system computes the world rectangle
//! PreparePresentation  sync_live_battle_system, refresh_hud_model, refresh_battle_fog,
//!                      apply_battle_fog, refresh_hud_focus, collect_battle_feed
//! ```
//! The bars are egui panels drawn before [`lab_ui_system`](crate::lab_ui_system) measures what is
//! left, so that rectangle is both the camera viewport and the hit test
//! [`sample_input_ownership`](crate::input_policy::sample_input_ownership) applies: a press on a
//! panel cannot reach the world, with no second mechanism to keep in step.
//!
//! This module depends on `battle_input` and `battle_pipeline`, never the reverse. The HUD reads
//! [`HudModel`](model::HudModel), which is filled from a viewer-scoped
//! [`PlayerView`](sim_types::PlayerView), and never reads the world while drawing.
//!
//! # Invariants
//! - Every system here runs only under the `Player` profile, so `Sandbox` is untouched.
//! - Nothing here names a unit or structure: all text comes from catalog data.
//! - [`HudGeometry`] holds only rectangles drawn on the current frame.

pub mod catalog;
pub mod feed;
pub mod fog;
pub mod model;
pub mod panels;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::battle_pipeline::fog::BattleFog;
use crate::battle_pipeline::BattleSet;
use crate::battle_presentation::BattleProfile;

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
            .init_resource::<model::HudModel>()
            .init_resource::<feed::BattleFeed>()
            .add_systems(
                Update,
                (
                    panels::battle_hud_system
                        .before(crate::lab_ui_system)
                        .in_set(BattleSet::CollectIntent),
                    (
                        model::refresh_hud_model,
                        fog::refresh_battle_fog,
                        fog::apply_battle_fog,
                        model::refresh_hud_focus,
                        feed::collect_battle_feed,
                    )
                        .chain()
                        .after(crate::sync_live_battle_system)
                        .in_set(BattleSet::PreparePresentation),
                )
                    .run_if(resource_equals(BattleProfile::Player)),
            );
    }
}
