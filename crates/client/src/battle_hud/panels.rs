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
//! explains. The feed and the note on a refused site float over the world. Neither takes input,
//! so a click on them lands in the world beneath.
//!
//! A click is recorded in [`HudIntent`], and [`actions`](super::actions) carries it out.
//!
//! # Invariants
//! - No panel is resizable, so the world rectangle changes only with the window.
//! - A failure is always shown, even when no battle exists to draw panels for.
//! - No text is formatted here: every string is cached in the model or the feed, or is constant.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim_types::{EntityId, UnitOrder};

use super::actions::HudIntent;
use super::card::{CardAction, CardSlot, CommandCard, SlotText};
use super::feed::{BattleFeed, FeedTone};
use super::model::{FocusedUnit, HudModel};
use super::placement::PlacementPreview;
use super::queue::{item_label, QueuePanel};
use super::HudGeometry;
use crate::battle_input::command_mode::CommandMode;
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

/// Width of a queue row's button, in egui points.
const QUEUE_ROW_WIDTH: f32 = 170.0;

/// Width of the button cancelling a foundation, in egui points.
const CANCEL_BUTTON_WIDTH: f32 = 120.0;

/// Positions on one row of the command card.
const CARD_COLUMNS: usize = 5;

/// Rows of the command card, counting the row of Stop and Hold.
const CARD_ROWS: usize = 4;

/// Gap between two command card buttons, in egui points.
const CARD_GAP: f32 = 2.0;

/// Gap between the top of the world and the note on a refused site, in egui points.
const SITE_NOTE_MARGIN: f32 = 12.0;

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
    mut intent: ResMut<'_, HudIntent>,
    model: Res<'_, HudModel>,
    feed: Res<'_, BattleFeed>,
    card: Res<'_, CommandCard>,
    queue: Res<'_, QueuePanel>,
    preview: Res<'_, PlacementPreview>,
    mode: Res<'_, CommandMode>,
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
        let surfaces = CommandSurfaces {
            model: &model,
            card: &card,
            queue: &queue,
            mode: *mode,
            cancelling: queue
                .structure
                .is_some_and(|structure| cancel_pending(&session, structure)),
        };
        draw_command_panel(ctx, &surfaces, &mut intent, &mut geometry);
        draw_feed(ctx, &feed, &mut geometry);
        if let Some(reason) = preview.refusal() {
            draw_site_note(ctx, reason, &mut geometry);
        }
    }
}

fn visible(response: &egui::Response, clip: egui::Rect) -> Option<egui::Rect> {
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

/// Whether a `CancelProduction` for `structure` still waits for its tick.
///
/// While paused, a second click would address a job index the first has yet to shift.
fn cancel_pending(session: &BattleSession, structure: EntityId) -> bool {
    session.commands.pending_commands().any(|(_, command)| {
        matches!(command.order, UnitOrder::CancelProduction { .. })
            && command.entities.contains(&structure)
    })
}

/// What the command panel draws.
struct CommandSurfaces<'a> {
    model: &'a HudModel,
    card: &'a CommandCard,
    queue: &'a QueuePanel,
    mode: CommandMode,
    cancelling: bool,
}

/// Draws the three columns of the command panel: selection, queue and card.
fn draw_command_panel(
    ctx: &egui::Context,
    surfaces: &CommandSurfaces<'_>,
    intent: &mut HudIntent,
    geometry: &mut HudGeometry,
) {
    let panel = egui::TopBottomPanel::bottom("battle_command")
        .exact_height(COMMAND_PANEL_HEIGHT)
        .resizable(false)
        .show(ctx, |ui| {
            ui.columns_const(|[selection, queue, card]| {
                geometry.selection_column = Some(selection.max_rect());
                geometry.queue_column = Some(queue.max_rect());
                geometry.card_column = Some(card.max_rect());
                draw_selection(selection, surfaces.model);
                draw_queue(queue, surfaces, intent, geometry);
                draw_card(card, surfaces, intent, geometry);
            });
        });
    geometry.bottom = Some(panel.response.rect);
}

/// Draws what the selected structures are making, or how far a foundation has come.
fn draw_queue(
    ui: &mut egui::Ui,
    surfaces: &CommandSurfaces<'_>,
    intent: &mut HudIntent,
    geometry: &mut HudGeometry,
) {
    let panel = surfaces.queue;
    if let Some(progress) = panel.foundation {
        ui.add(
            egui::ProgressBar::new(progress)
                .desired_height(BAR_HEIGHT)
                .show_percentage(),
        );
        let cancel = ui
            .add(egui::Button::new("Cancel build").min_size(egui::vec2(CANCEL_BUTTON_WIDTH, 0.0)));
        geometry.queue_cancel = visible(&cancel, ui.clip_rect());
        if cancel.clicked() {
            intent.action = Some(CardAction::Order(UnitOrder::CancelConstruction));
        }
        return;
    }
    for line in panel.lines() {
        ui.label(line.text.as_str());
    }
    let Some(structure) = panel.structure else {
        return;
    };
    for (position, row) in panel.rows.iter().enumerate() {
        let Some(row) = row else {
            continue;
        };
        ui.horizontal(|ui| {
            let button = egui::Button::new(item_label(surfaces.model, row.item))
                .truncate()
                .min_size(egui::vec2(QUEUE_ROW_WIDTH, 0.0));
            let click = ui.add_enabled(!surfaces.cancelling, button);
            geometry.queue[position] = visible(&click, ui.clip_rect());
            if click.clicked() {
                intent.cancel_job = Some((structure, row.job_index));
            }
            let mut bar = egui::ProgressBar::new(row.progress);
            if position == 0 && panel.exit_blocked {
                bar = bar.text("Exit blocked");
            }
            ui.add(bar);
        });
    }
    if !panel.overflow.is_empty() {
        ui.small(panel.overflow.as_str());
    }
}

/// One button of the card, in `rect`. Returns what clicking it does.
fn draw_slot(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    slot: &CardSlot,
    text: &SlotText,
    mode: CommandMode,
) -> (egui::Response, Option<CardAction>) {
    let button = egui::Button::new(text.caption.as_str())
        .truncate()
        .selected(slot.action == CardAction::Mode(mode))
        .min_size(rect.size());
    let mut response = ui
        .add_enabled_ui(slot.enabled, |ui| ui.put(rect, button))
        .inner;
    if !text.tooltip.is_empty() {
        response = response
            .on_hover_text(text.tooltip.as_str())
            .on_disabled_hover_text(text.tooltip.as_str());
    }
    let action = response.clicked().then(|| slot.action.clone());
    (response, action)
}

/// Draws the card as a grid: three rows of positions, then Stop and Hold.
fn draw_card(
    ui: &mut egui::Ui,
    surfaces: &CommandSurfaces<'_>,
    intent: &mut HudIntent,
    geometry: &mut HudGeometry,
) {
    let area = ui.max_rect();
    #[expect(
        clippy::cast_precision_loss,
        reason = "both counts are small constants"
    )]
    let cell = egui::vec2(
        area.width() / CARD_COLUMNS as f32,
        area.height() / CARD_ROWS as f32,
    );
    let rect_of = |position: usize| {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a grid position is a small number"
        )]
        let corner = egui::vec2(
            (position % CARD_COLUMNS) as f32 * cell.x,
            (position / CARD_COLUMNS) as f32 * cell.y,
        );
        egui::Rect::from_min_size(area.min + corner, cell - egui::Vec2::splat(CARD_GAP))
    };
    let (card, mode) = (surfaces.card, surfaces.mode);
    for (position, (slot, text)) in card.slots.iter().zip(&card.text).enumerate() {
        if let Some(slot) = slot {
            let (response, action) = draw_slot(ui, rect_of(position), slot, text, mode);
            geometry.card[position] = visible(&response, ui.clip_rect());
            intent.action = action.or(intent.action.take());
        }
    }
    let fixed_row = CARD_COLUMNS * (CARD_ROWS - 1);
    for (position, (slot, text)) in card.fixed.iter().zip(&card.fixed_text).enumerate() {
        if let Some(slot) = slot {
            let (response, action) = draw_slot(ui, rect_of(fixed_row + position), slot, text, mode);
            geometry.card_fixed[position] = visible(&response, ui.clip_rect());
            intent.action = action.or(intent.action.take());
        }
    }
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

/// Says why the site under the pointer is refused, on an area that takes no input.
fn draw_site_note(ctx: &egui::Context, reason: &str, geometry: &mut HudGeometry) {
    let world = ctx.available_rect();
    let area = egui::Area::new(egui::Id::new("battle_site_note"))
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(world.center_top() + egui::vec2(0.0, SITE_NOTE_MARGIN))
        .interactable(false)
        .show(ctx, |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(reason).color(REFUSAL_COLOR))
                    .selectable(false),
            );
        });
    geometry.site_note = Some(area.response.rect);
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
