//! Shows the minimap image in the HUD (plan `2026-12-01-minimap` D-14).
//!
//! # Responsibility
//! Draws the image the minimap node wrote, as an `egui::Image` of the texture
//! [`MinimapFrame::texture`](super::MinimapFrame::texture) names, in one of two places:
//! - inside the command panel, as a fixed-width left part beside the selection, queue and card
//!   columns ([`split_command_panel`], [`draw_in_panel`]). The panel keeps its height, so the
//!   world rectangle does not change;
//! - alone, in a bottom-left area that reserves no room, when a layout leaves the command panel
//!   out but keeps the minimap ([`draw_alone`]).
//!
//! It returns the image's response, whose square, in egui points, the HUD records as
//! [`HudGeometry::minimap`](crate::battle_hud::HudGeometry::minimap), and [`read_intent`] turns
//! the pointer on it into a [`MinimapIntent`] (plan D-7):
//! - a left press, or a left drag that began on the image, with no command mode armed centres the
//!   camera on the point (`look_at`); it never selects;
//! - a left press with a mode armed aims the mode there (`aim_left`);
//! - a right press orders there, or cancels an armed mode (`aim_right`);
//! - Shift at the press queues.
//!
//! Presses act on the press, as in the world. egui owns the pointer over the image, so the world
//! sees none of these presses, and egui's drag capture keeps a drag begun on the image there until
//! release.
//!
//! # Invariants
//! - Nothing is drawn until the image and its texture exist; the left part is still reserved, so
//!   the columns never move when the image appears.
//! - The square is `side_points` wide, at most the room it is given, and centred in it.

use bevy::ecs::system::SystemParam;
use bevy::math::Vec2;
use bevy::prelude::{ButtonInput, KeyCode, Res, ResMut};
use bevy_egui::egui;

use super::style::MinimapStyleStatus;
use super::{MinimapFrame, MinimapIntent};

/// What the HUD reads and writes of the minimap: absent outside the player's profile.
#[derive(SystemParam)]
pub struct MinimapHud<'w> {
    /// The retained frame, whose image the HUD shows.
    pub frame: Option<Res<'w, MinimapFrame>>,
    /// Whether the style loaded.
    pub status: Option<Res<'w, MinimapStyleStatus>>,
    /// This frame's pointer intent, written by the HUD.
    pub intent: Option<ResMut<'w, MinimapIntent>>,
    /// The keyboard, for Shift: the world reads it from here too.
    pub keyboard: Res<'w, ButtonInput<KeyCode>>,
}

/// Gap between the minimap's part of the command panel and the columns, in egui points.
pub const MINIMAP_GAP: f32 = 6.0;

/// Splits the command panel's room `full` into the minimap's left part and the room left for
/// the columns. With no minimap the columns take everything.
#[must_use]
pub fn split_command_panel(
    full: egui::Rect,
    side_points: Option<f32>,
) -> (Option<egui::Rect>, egui::Rect) {
    let Some(side) = side_points.filter(|side| *side > 0.0) else {
        return (None, full);
    };
    let width = side.min(full.width());
    let left = egui::Rect::from_min_size(full.min, egui::vec2(width, full.height()));
    let rest = egui::Rect::from_min_max(
        egui::pos2((left.max.x + MINIMAP_GAP).min(full.max.x), full.min.y),
        full.max,
    );
    (Some(left), rest)
}

/// The square of `frame`'s image centred in `room`, if the image exists.
fn square_in(frame: &MinimapFrame, room: egui::Rect) -> Option<(egui::TextureId, egui::Rect)> {
    let texture = frame.texture()?;
    let side = frame.side_points().min(room.width()).min(room.height());
    (side > 0.0).then(|| {
        (
            texture,
            egui::Rect::from_center_size(room.center(), egui::Vec2::splat(side)),
        )
    })
}

/// Draws the image centred in `left`, the minimap's part of the command panel. Returns its
/// response.
pub fn draw_in_panel(
    ui: &mut egui::Ui,
    frame: &MinimapFrame,
    left: egui::Rect,
) -> Option<egui::Response> {
    let (texture, square) = square_in(frame, left)?;
    Some(ui.put(square, image(texture, square.size())))
}

/// Draws the image alone at the bottom left of the window, on an area that reserves no room.
/// Returns its response.
#[must_use]
pub fn draw_alone(ctx: &egui::Context, frame: &MinimapFrame) -> Option<egui::Response> {
    let texture = frame.texture()?;
    let side = frame.side_points();
    let corner = ctx.screen_rect().left_bottom();
    let area = egui::Area::new(egui::Id::new("battle_minimap"))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(corner)
        .show(ctx, |ui| ui.add(image(texture, egui::Vec2::splat(side))));
    Some(area.inner)
}

/// The image widget of `texture` at `size` points, sensing clicks and drags.
fn image(texture: egui::TextureId, size: egui::Vec2) -> egui::Image<'static> {
    egui::Image::new(egui::load::SizedTexture::new(texture, size))
        .sense(egui::Sense::click_and_drag())
}

/// The intent of this frame's pointer on the minimap image `response`, with `armed` true while a
/// command mode is armed and `shift` while Shift is held (read as the world reads it). Points are
/// map points in world units, clamped to the camera's bound.
#[must_use]
pub fn read_intent(
    response: &egui::Response,
    frame: &MinimapFrame,
    armed: bool,
    shift: bool,
) -> MinimapIntent {
    let mut intent = MinimapIntent::default();
    let Some(layout) = frame.layout() else {
        return intent;
    };
    let rect = response.rect;
    let (pressed_left, pressed_right, pointer) = response.ctx.input(|input| {
        (
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.button_pressed(egui::PointerButton::Secondary),
            input.pointer.interact_pos(),
        )
    });
    let Some(pointer) = pointer.filter(|_| rect.width() > 0.0) else {
        return intent;
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "the image side is a window's size in pixels"
    )]
    let pixels_per_point = frame.side_px() as f32 / rect.width();
    let pixel = Vec2::new(pointer.x - rect.min.x, pointer.y - rect.min.y) * pixels_per_point;
    let point = layout.to_world_clamped(pixel);
    let hovered = response.hovered() || response.contains_pointer();
    let left_held_here = response.dragged_by(egui::PointerButton::Primary);
    intent.shift = shift;
    if hovered && pressed_right {
        intent.aim_right = Some(point);
    }
    if armed {
        if hovered && pressed_left {
            intent.aim_left = Some(point);
        }
    } else if (hovered && pressed_left) || left_held_here {
        intent.look_at = Some(point);
    }
    intent
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;

    #[test]
    fn the_minimap_takes_a_left_part_and_the_columns_the_rest() {
        let full = egui::Rect::from_min_size(egui::pos2(8.0, 542.0), egui::vec2(1264.0, 176.0));
        let (left, rest) = split_command_panel(full, Some(172.0));
        let left = left.expect("a minimap part");
        assert_eq!(left.width(), 172.0);
        assert_eq!(left.height(), 176.0);
        assert_eq!(rest.min.x, left.max.x + MINIMAP_GAP);
        assert_eq!(rest.max, full.max);
    }

    #[test]
    fn without_a_minimap_the_columns_take_everything() {
        let full = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 50.0));
        assert_eq!(split_command_panel(full, None), (None, full));
    }
}
