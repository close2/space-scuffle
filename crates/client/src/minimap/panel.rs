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
//! It returns the square it drew, in egui points, which the HUD records as
//! [`HudGeometry::minimap`](crate::battle_hud::HudGeometry::minimap).
//!
//! # Invariants
//! - Nothing is drawn until the image and its texture exist; the left part is still reserved, so
//!   the columns never move when the image appears.
//! - The square is `side_points` wide, at most the room it is given, and centred in it.

use bevy_egui::egui;

use super::MinimapFrame;

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

/// Draws the image centred in `left`, the minimap's part of the command panel. Returns the square
/// drawn.
pub fn draw_in_panel(
    ui: &mut egui::Ui,
    frame: &MinimapFrame,
    left: egui::Rect,
) -> Option<egui::Rect> {
    let (texture, square) = square_in(frame, left)?;
    ui.put(square, image(texture, square.size()));
    Some(square)
}

/// Draws the image alone at the bottom left of the window, on an area that reserves no room.
/// Returns the square drawn.
#[must_use]
pub fn draw_alone(ctx: &egui::Context, frame: &MinimapFrame) -> Option<egui::Rect> {
    let texture = frame.texture()?;
    let side = frame.side_points();
    let corner = ctx.screen_rect().left_bottom();
    let area = egui::Area::new(egui::Id::new("battle_minimap"))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(corner)
        .show(ctx, |ui| ui.add(image(texture, egui::Vec2::splat(side))));
    Some(area.inner.rect)
}

/// The image widget of `texture` at `size` points.
fn image(texture: egui::TextureId, size: egui::Vec2) -> egui::Image<'static> {
    egui::Image::new(egui::load::SizedTexture::new(texture, size))
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
