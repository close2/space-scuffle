//! The showcase's text: title card, captions, status card and badge, painted by egui in the engine.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, decision D-6. [`draw_overlay`] paints the
//! current scene's overlay on `egui::Order::Foreground`, above the production HUD, so the text is in
//! the frames the engine renders and is read back with the rest of the image. ffmpeg draws nothing.
//!
//! # Layout
//! Sizes are egui points designed for a 720-line frame; the take sets egui's scale factor to
//! `height / 720`, so another resolution scales the same layout. Fonts are egui's bundled ones:
//! Ubuntu-Light for text and Hack for the revision footer; nothing is added to the repository.
//!
//! # Invariants
//! - Every fade is a function of the scene's frame index ([`fade_alpha`]), never of egui animation
//!   time, so a frame's pixels depend only on its index and the simulation.
//! - The overlay reads its state; it changes nothing in the game.

use super::cards::{StatusCard, TitleCard};
use super::storyboard::Overlay;
use bevy::prelude::*;
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, Vec2 as EVec2};
use bevy_egui::EguiContexts;

/// Frames over which an overlay fades in at the start of its scene and out at its end.
pub const FADE_FRAMES: u32 = 10;

/// Lines of the reference layout; egui's scale factor is the frame height over this.
pub const LAYOUT_LINES: f32 = 720.0;

/// Opacity of a card's full-frame backdrop.
const CARD_BACKDROP: f32 = 0.7;

/// Opacity of the caption band.
const CAPTION_BACKDROP: f32 = 0.6;

/// The opacity of an overlay at `frame` of a scene of `length` frames (`None` while the length is
/// not known, as in an `UntilCombat` scene): a linear ramp over [`FADE_FRAMES`] at each end.
#[must_use]
pub fn fade_alpha(frame: u32, length: Option<u32>) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "frame counts are at most 3,000, exact in f32"
    )]
    let ramp = |frames: u32| (frames as f32 / FADE_FRAMES as f32).min(1.0);
    let fade_in = ramp(frame);
    let fade_out = length.map_or(1.0, |length| {
        ramp(length.saturating_sub(1).saturating_sub(frame))
    });
    fade_in.min(fade_out)
}

/// What the overlay shows in the current frame; the take updates it once per frame.
#[derive(Resource, Debug, Clone, Default)]
pub struct OverlayState {
    /// The scene's overlay, or `None` while loading or draining.
    pub overlay: Option<Overlay>,
    /// The scene's badge.
    pub badge: Option<String>,
    /// Frame within the scene.
    pub scene_frame: u32,
    /// The scene's length, when known in advance.
    pub length: Option<u32>,
}

/// The generated card texts and the revision footer.
#[derive(Resource, Debug, Clone)]
pub struct CardTexts {
    /// The title card.
    pub title: TitleCard,
    /// The status card.
    pub status: StatusCard,
    /// Short commit hash.
    pub revision: String,
    /// Commit date, `YYYY-MM-DD`.
    pub revision_date: String,
    /// The adapter that renders, filled in once the renderer exists.
    pub adapter: String,
}

impl CardTexts {
    /// The footer of the title and status cards.
    #[must_use]
    pub fn footer(&self) -> String {
        format!(
            "Real engine output · rendered on {} · {} · {}",
            self.adapter, self.revision, self.revision_date
        )
    }
}

fn white(alpha: f32, level: u8) -> Color32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "alpha is clamped to 0..=1 before scaling to a byte"
    )]
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(level, level, level, a)
}

fn accent(alpha: f32) -> Color32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "alpha is clamped to 0..=1 before scaling to a byte"
    )]
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(255, 196, 92, a)
}

fn black(alpha: f32) -> Color32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "alpha is clamped to 0..=1 before scaling to a byte"
    )]
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_black_alpha(a)
}

/// Lays out `text` wrapped at `width` points.
fn galley(
    ctx: &egui::Context,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    ctx.fonts(|fonts| fonts.layout(text.to_owned(), font, color, width))
}

/// Paints `text` with its top-left or top-centre at `at`; returns its height in points.
fn text(
    painter: &egui::Painter,
    ctx: &egui::Context,
    at: Pos2,
    align_center: bool,
    line: (&str, FontId, Color32, f32),
) -> f32 {
    let (content, font, color, width) = line;
    let layout = galley(ctx, content, font, color, width);
    let size = layout.size();
    let position = if align_center {
        Pos2::new(at.x - size.x / 2.0, at.y)
    } else {
        at
    };
    painter.galley(position, layout, color);
    size.y
}

/// Draws the current overlay. Runs in `Update`, inside egui's pass.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn draw_overlay(
    mut contexts: EguiContexts<'_, '_>,
    state: Res<'_, OverlayState>,
    cards: Res<'_, CardTexts>,
) {
    let Some(overlay) = &state.overlay else {
        return;
    };
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let alpha = fade_alpha(state.scene_frame, state.length);
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("showcase_overlay"));
    let painter = ctx.layer_painter(layer);
    let screen = ctx.screen_rect();
    match overlay {
        Overlay::None => {}
        Overlay::TitleCard => title_card(&painter, ctx, screen, &cards, alpha),
        Overlay::Caption { title, body, .. } => {
            caption(&painter, ctx, screen, (title, body), alpha);
        }
        Overlay::StatusCard => status_card(&painter, ctx, screen, &cards, alpha),
    }
    if let Some(badge) = &state.badge {
        draw_badge(&painter, ctx, screen, badge);
    }
}

fn title_card(
    painter: &egui::Painter,
    ctx: &egui::Context,
    screen: Rect,
    cards: &CardTexts,
    alpha: f32,
) {
    painter.rect_filled(screen, 0.0, black(CARD_BACKDROP * alpha));
    let center = screen.center().x;
    let mut y = screen.top() + screen.height() * 0.30;
    y += text(
        painter,
        ctx,
        Pos2::new(center, y),
        true,
        (
            &cards.title.name,
            FontId::proportional(64.0),
            white(alpha, 255),
            1_000.0,
        ),
    );
    let rule = Rect::from_center_size(Pos2::new(center, y + 14.0), EVec2::new(120.0, 2.0));
    painter.rect_filled(rule, 0.0, accent(alpha));
    y += 34.0;
    text(
        painter,
        ctx,
        Pos2::new(center, y),
        true,
        (
            &cards.title.pitch,
            FontId::proportional(24.0),
            white(alpha, 230),
            820.0,
        ),
    );
    footer(painter, ctx, screen, cards, alpha);
}

fn footer(
    painter: &egui::Painter,
    ctx: &egui::Context,
    screen: Rect,
    cards: &CardTexts,
    alpha: f32,
) {
    let at = Pos2::new(screen.center().x, screen.bottom() - 40.0);
    text(
        painter,
        ctx,
        at,
        true,
        (
            &cards.footer(),
            FontId::monospace(13.0),
            white(alpha, 200),
            1_200.0,
        ),
    );
}

fn caption(
    painter: &egui::Painter,
    ctx: &egui::Context,
    screen: Rect,
    lines: (&str, &str),
    alpha: f32,
) {
    let (title, body) = lines;
    let width = 760.0;
    let heading = galley(
        ctx,
        title,
        FontId::proportional(28.0),
        white(alpha, 255),
        width,
    );
    let detail = galley(
        ctx,
        body,
        FontId::proportional(18.0),
        white(alpha, 225),
        width,
    );
    let inner = EVec2::new(
        heading.size().x.max(detail.size().x),
        heading.size().y + 6.0 + detail.size().y,
    );
    let padding = EVec2::new(28.0, 16.0);
    let top = screen.top() + 64.0;
    let band = Rect::from_min_size(
        Pos2::new(screen.center().x - inner.x / 2.0 - padding.x, top),
        inner + padding * 2.0,
    );
    painter.rect_filled(band, 10.0, black(CAPTION_BACKDROP * alpha));
    let rule = Rect::from_min_size(
        band.left_top() + EVec2::new(0.0, 10.0),
        EVec2::new(4.0, band.height() - 20.0),
    );
    painter.rect_filled(rule, 2.0, accent(alpha));
    let heading_height = heading.size().y;
    let heading_at = Pos2::new(screen.center().x - heading.size().x / 2.0, top + padding.y);
    painter.galley(heading_at, heading, white(alpha, 255));
    let detail_at = Pos2::new(
        screen.center().x - detail.size().x / 2.0,
        top + padding.y + heading_height + 6.0,
    );
    painter.galley(detail_at, detail, white(alpha, 225));
}

/// One line of a status card column: text, size in points, colour, and the gap after it.
type ColumnLine = (String, f32, Color32, f32);

/// Paints `lines` top-down from `(x, top)`, wrapped at `width` points.
fn column(
    painter: &egui::Painter,
    ctx: &egui::Context,
    at: Pos2,
    width: f32,
    lines: &[ColumnLine],
) {
    let mut y = at.y;
    for (content, size, color, gap) in lines {
        y += text(
            painter,
            ctx,
            Pos2::new(at.x, y),
            false,
            (content, FontId::proportional(*size), *color, width),
        );
        y += gap;
    }
}

fn status_card(
    painter: &egui::Painter,
    ctx: &egui::Context,
    screen: Rect,
    cards: &CardTexts,
    alpha: f32,
) {
    painter.rect_filled(screen, 0.0, black(CARD_BACKDROP * alpha));
    let status = &cards.status;
    let width = 470.0;
    let gap = 60.0;
    let top = screen.top() + 150.0;
    let mut left = vec![
        ("Where it is".to_owned(), 34.0, accent(alpha), 14.0),
        (status.counts_line(), 22.0, white(alpha, 255), 14.0),
    ];
    for (label, names) in [
        ("Partial", &status.partial),
        ("Not built", &status.not_built),
    ] {
        if !names.is_empty() {
            left.push((
                format!("{label}: {}", names.join(", ")),
                17.0,
                white(alpha, 215),
                10.0,
            ));
        }
    }
    let mut right = vec![
        ("Where it is heading".to_owned(), 34.0, accent(alpha), 14.0),
        (
            format!("Intent: {}", status.intent),
            17.0,
            white(alpha, 215),
            14.0,
        ),
    ];
    if !status.plans.is_empty() {
        right.push(("Plans in progress".to_owned(), 20.0, white(alpha, 255), 6.0));
        for plan in &status.plans {
            right.push((format!("•  {plan}"), 17.0, white(alpha, 215), 4.0));
        }
    }
    let center = screen.center().x;
    column(
        painter,
        ctx,
        Pos2::new(center - width - gap / 2.0, top),
        width,
        &left,
    );
    column(
        painter,
        ctx,
        Pos2::new(center + gap / 2.0, top),
        width,
        &right,
    );
    footer(painter, ctx, screen, cards, alpha);
}

fn draw_badge(painter: &egui::Painter, ctx: &egui::Context, screen: Rect, badge: &str) {
    let layout = galley(
        ctx,
        badge,
        FontId::proportional(18.0),
        Color32::WHITE,
        400.0,
    );
    let padding = EVec2::new(14.0, 7.0);
    let size = layout.size() + padding * 2.0;
    let anchor = Pos2::new(screen.right() - 24.0, screen.top() + 64.0);
    let rect = Align2::RIGHT_TOP.anchor_size(anchor, size);
    painter.rect_filled(rect, 8.0, black(0.65));
    painter.rect_stroke(
        rect,
        8.0_f32,
        egui::Stroke::new(1.5_f32, accent(1.0)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + padding, layout, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_fade_is_a_function_of_the_frame_index() {
        let length = Some(100);
        assert_eq!(fade_alpha(0, length), 0.0);
        assert_eq!(fade_alpha(5, length), 0.5);
        assert_eq!(fade_alpha(10, length), 1.0);
        assert_eq!(fade_alpha(89, length), 1.0);
        assert_eq!(fade_alpha(94, length), 0.5);
        assert_eq!(fade_alpha(99, length), 0.0);
        // An open-ended scene only fades in.
        assert_eq!(fade_alpha(2_000, None), 1.0);
        assert_eq!(fade_alpha(5, None), 0.5);
        // The same index gives the same alpha, bit for bit.
        assert_eq!(
            fade_alpha(7, length).to_bits(),
            fade_alpha(7, length).to_bits()
        );
    }
}
