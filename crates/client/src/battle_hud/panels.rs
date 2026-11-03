//! Draws the HUD with egui, and nothing else.
//!
//! # Responsibility
//! [`battle_hud_system`] is the one entry point and stays thin: it hands data to one small draw
//! function per surface. Every draw function reads its arguments and, at most, records rectangles
//! and returns what was clicked.
//!
//! # How it fits
//! Runs before [`lab_ui_system`](crate::lab_ui_system), which measures the room the panels leave.
//! That room becomes the camera viewport and the world's hit test, as [`battle_hud`](super)
//! explains. The feed floats too, but it takes no input, so a click on it lands in the world
//! beneath. The one floating surface that takes clicks is the [`result`](super::result) window,
//! which draws itself.
//!
//! # Invariants
//! - No panel is resizable, so the world rectangle changes only with the window.
//! - A failure is always shown, even when no battle exists to draw panels for.
//! - No text is formatted here: every string is cached in the model or the feed, or is constant.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::feed::{BattleFeed, FeedTone};
use super::model::{FocusedUnit, HudModel};
use super::HudGeometry;
use crate::battle_pipeline::session::BattleSession;
use crate::lab_ui::LabState;
use crate::media::ClientMediaStatus;
use crate::player_presentation::PresentationStatus;

/// Height of the resource bar, in egui points.
pub const RESOURCE_BAR_HEIGHT: f32 = 28.0;

/// Height of the command panel, in egui points.
pub const COMMAND_PANEL_HEIGHT: f32 = 180.0;

/// Slowest simulation speed the bar offers, as a multiple of real time.
pub const MIN_SIMULATION_SPEED: u32 = 1;

/// Fastest simulation speed the bar offers, as a multiple of real time.
pub const MAX_SIMULATION_SPEED: u32 = 4;

/// Speed captions, indexed by speed less [`MIN_SIMULATION_SPEED`].
const SPEED_LABELS: [&str; 4] = ["1×", "2×", "3×", "4×"];

/// Height at which failure text starts to scroll, in egui points.
const FAILURE_MAX_HEIGHT: f32 = 96.0;

/// Widest a feed line grows before wrapping, in egui points.
const FEED_WIDTH: f32 = 420.0;

/// Gap between the feed and the edges of the world rectangle, in egui points.
const FEED_MARGIN: f32 = 8.0;

/// Height of a health or shield bar, in egui points.
const BAR_HEIGHT: f32 = 10.0;

const HEALTH_COLOR: egui::Color32 = egui::Color32::from_rgb(70, 190, 90);
const SHIELD_COLOR: egui::Color32 = egui::Color32::from_rgb(80, 150, 230);
const REFUSAL_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 140, 120);
const NOTICE_COLOR: egui::Color32 = egui::Color32::from_rgb(240, 210, 130);
const MILESTONE_COLOR: egui::Color32 = egui::Color32::from_rgb(140, 230, 150);

// PERFORMANCE RATIONALE:
// Why: egui is an immediate-mode library: it lays out every widget and copies its text every
// frame, and that allocation is inside the library, where no caller can remove it.
// What: This code adds none of its own. Text arrives cached in `HudModel` and `BattleFeed`, and
// refreshing those is what the no-allocation rule of the charter governs.
// Trade-off: A number on screen goes through a cached string rather than a `format!` at the widget.
/// Draws the HUD of the `Player` profile.
///
/// The panels wait for a viewer, because until then there is no match to show. A failure does not:
/// with no inspector, this is the only place the player can learn of one.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn battle_hud_system(
    mut contexts: EguiContexts<'_, '_>,
    mut lab: ResMut<'_, LabState>,
    mut geometry: ResMut<'_, HudGeometry>,
    model: Res<'_, HudModel>,
    feed: Res<'_, BattleFeed>,
    session: Res<'_, BattleSession>,
    media: Option<Res<'_, ClientMediaStatus>>,
    owner_status: Res<'_, PresentationStatus>,
) {
    *geometry = HudGeometry::default();
    // `lab_ui_system` reports a live window without a context.
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let ctx = &*ctx;

    let media_failed = media.as_ref().and_then(|status| status.failure()).is_some()
        || owner_status.failure_text().is_some();
    let failure = media
        .as_ref()
        .and_then(|status| status.failure_text())
        .or(owner_status.failure_text())
        .or(lab.battle_failure.as_deref());
    let failed = failure.is_some();
    // A new world can recover a simulation failure. A missing or invalid media bundle cannot be
    // repaired by resetting simulation state.
    let resettable = lab.battle_failure.is_some() && !media_failed;
    let reset =
        failure.is_some_and(|message| draw_failure(ctx, message, resettable, &mut geometry));
    if reset {
        lab.battle_reset_requested = true;
    }

    if model.viewer.is_some() {
        let enabled = session.is_ready() && !failed;
        draw_resource_bar(ctx, &model, &mut lab, enabled, &mut geometry);
        draw_command_panel(ctx, &model, &mut geometry);
        draw_feed(ctx, &feed, &mut geometry);
    }
}

/// The part of `response` inside `clip`, or `None` if nothing of it shows.
pub(super) fn visible(response: &egui::Response, clip: egui::Rect) -> Option<egui::Rect> {
    let rect = response.rect.intersect(clip);
    rect.is_positive().then_some(rect)
}

/// Draws the failure banner, and returns whether its reset button was clicked.
fn draw_failure(
    ctx: &egui::Context,
    message: &str,
    resettable: bool,
    geometry: &mut HudGeometry,
) -> bool {
    let mut reset = false;
    let panel = egui::TopBottomPanel::top("battle_failure")
        .resizable(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("battle_failure_text")
                .max_height(FAILURE_MAX_HEIGHT)
                .show(ui, |ui| {
                    ui.colored_label(egui::Color32::LIGHT_RED, message);
                });
            if resettable {
                let button = ui.button("Reset failed battle");
                geometry.reset = visible(&button, ui.clip_rect());
                reset = button.clicked();
            }
        });
    geometry.failure = Some(panel.response.rect);
    reset
}

/// Draws the bank, the clock, and the pause and speed controls.
fn draw_resource_bar(
    ctx: &egui::Context,
    model: &HudModel,
    lab: &mut LabState,
    enabled: bool,
    geometry: &mut HudGeometry,
) {
    let panel = egui::TopBottomPanel::top("battle_resources")
        .exact_height(RESOURCE_BAR_HEIGHT)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(model.resource_line.as_str());
                ui.separator();
                ui.label(model.clock_line.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        draw_speed_controls(ui, lab, geometry);
                    });
                });
            });
        });
    geometry.top = Some(panel.response.rect);
}

/// Lays out, from the right edge inwards: speed up, the speed, speed down, pause.
fn draw_speed_controls(ui: &mut egui::Ui, lab: &mut LabState, geometry: &mut HudGeometry) {
    let up = ui.add_enabled(
        lab.simulation_speed < MAX_SIMULATION_SPEED,
        egui::Button::new("+"),
    );
    geometry.speed_up = visible(&up, ui.clip_rect());
    if up.clicked() {
        lab.simulation_speed += 1;
    }
    let caption = lab
        .simulation_speed
        .checked_sub(MIN_SIMULATION_SPEED)
        .and_then(|index| SPEED_LABELS.get(index as usize));
    if let Some(caption) = caption {
        ui.label(*caption);
    }
    let down = ui.add_enabled(
        lab.simulation_speed > MIN_SIMULATION_SPEED,
        egui::Button::new("-"),
    );
    geometry.speed_down = visible(&down, ui.clip_rect());
    if down.clicked() {
        lab.simulation_speed -= 1;
    }
    let pause = ui.button(if lab.battle_paused { "Resume" } else { "Pause" });
    geometry.pause = visible(&pause, ui.clip_rect());
    if pause.clicked() {
        lab.battle_paused = !lab.battle_paused;
    }
}

/// Draws the three columns of the command panel: selection, queue and card.
fn draw_command_panel(ctx: &egui::Context, model: &HudModel, geometry: &mut HudGeometry) {
    let panel = egui::TopBottomPanel::bottom("battle_command")
        .exact_height(COMMAND_PANEL_HEIGHT)
        .resizable(false)
        .show(ctx, |ui| {
            ui.columns_const(|[selection, queue, card]| {
                geometry.selection_column = Some(selection.max_rect());
                geometry.queue_column = Some(queue.max_rect());
                geometry.card_column = Some(card.max_rect());
                draw_selection(selection, model);
            });
        });
    geometry.bottom = Some(panel.response.rect);
}

/// Draws what is selected: one unit with its bars, or several as names and counts.
fn draw_selection(ui: &mut egui::Ui, model: &HudModel) {
    if let Some(focus) = model.focus {
        draw_focused_unit(ui, model, focus);
    } else {
        egui::ScrollArea::vertical()
            .id_salt("battle_selection_groups")
            .show(ui, |ui| {
                for group in model.selection_groups() {
                    ui.label(group.label.as_str());
                }
            });
    }
}

fn draw_focused_unit(ui: &mut egui::Ui, model: &HudModel, focus: FocusedUnit) {
    ui.strong(model.catalog.label(focus.unit_type));
    ui.small("Health");
    ui.add(
        egui::ProgressBar::new(focus.health)
            .desired_height(BAR_HEIGHT)
            .fill(HEALTH_COLOR),
    );
    if let Some(shield) = focus.shield {
        ui.small("Shield");
        ui.add(
            egui::ProgressBar::new(shield)
                .desired_height(BAR_HEIGHT)
                .fill(SHIELD_COLOR),
        );
    }
}

const fn tone_color(tone: FeedTone) -> egui::Color32 {
    match tone {
        FeedTone::Refusal => REFUSAL_COLOR,
        FeedTone::Notice => NOTICE_COLOR,
        FeedTone::Milestone => MILESTONE_COLOR,
    }
}

/// Draws the live feed lines at the bottom left of the world, on an area that takes no input.
fn draw_feed(ctx: &egui::Context, feed: &BattleFeed, geometry: &mut HudGeometry) {
    if feed.lines().next().is_none() {
        return;
    }
    let world = ctx.available_rect();
    let area = egui::Area::new(egui::Id::new("battle_feed"))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(world.left_bottom() + egui::vec2(FEED_MARGIN, -FEED_MARGIN))
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_max_width(FEED_WIDTH);
            for line in feed.lines() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(line.text.as_str()).color(tone_color(line.tone)),
                    )
                    .selectable(false),
                );
            }
        });
    geometry.feed = Some(area.response.rect);
}
