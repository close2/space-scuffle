//! The screen that tells the viewer how the match ended, and what to do next.
//!
//! # Responsibility
//! Owns what a [`MatchResult`] means to one viewer ([`viewer_result`]), the cached text of that
//! verdict ([`ResultScreen`]), and the window offering `Play again` and `Exit`.
//!
//! # How it fits
//! ```text
//! CollectIntent        battle_result_system, after the panels and before the world is measured
//! PreparePresentation  refresh_result_screen, after refresh_hud_model
//! ```
//! `Play again` asks for the reset the failure banner asks for, so
//! [`manage_battle_scene`](crate::runtime_battle::manage_battle_scene) commits a new generation and
//! every per-session buffer resets on it. This screen needs no reset of its own, because it is
//! derived on each frame from the presented snapshot, and a new session presents a match in
//! progress.
//!
//! The window floats over the world and reserves nothing, so the world rectangle cannot protect
//! it. It is an interactable area, and
//! [`sample_input_ownership`](crate::input_policy::sample_input_ownership) refuses the world any
//! pointer over one.
//!
//! # Invariants
//! - The verdict follows the presented snapshot, so it appears with the displayed tick, not a tick
//!   early, and the world keeps ticking behind it (D-12).
//! - The screen says who won in the viewer's terms and how long the match took, which the viewer
//!   is told anyway. It reads no other player's slot, so it reveals nothing fog hides.
//! - A failure banner replaces the window: a battle that stopped cannot offer its verdict.
//! - A frame on which the verdict did not change rewrites nothing and allocates nothing.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim_types::snapshot::PlayerStatusView;
use sim_types::{MatchResult, PlayerId, Tick, MAX_PLAYERS};

use super::model::{clock_seconds, write_clock, HudModel};
use super::panels::visible;
use super::HudGeometry;
use crate::battle_pipeline::session::BattleSession;
use crate::lab_ui::LabState;

/// Width of the result window, in egui points.
pub const RESULT_WINDOW_WIDTH: f32 = 240.0;

/// What a match's standing means to one viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerResult {
    /// The viewer is the last player standing.
    Victory,
    /// The match ended with another player standing.
    Defeat,
    /// The last players all fell on one tick.
    Draw,
    /// The viewer is out of a match that others still play.
    Eliminated,
}

impl ViewerResult {
    /// The word the screen shows.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Victory => "Victory",
            Self::Defeat => "Defeat",
            Self::Draw => "Draw",
            Self::Eliminated => "Eliminated",
        }
    }
}

/// What `result` means to `viewer`, or `None` while the viewer is still playing.
///
/// `players` is indexed by player slot, and only `viewer`'s own slot is read. A viewer outside the
/// slots has nothing to be defeated in, so an ongoing match shows them nothing.
///
/// A viewer who fell before a larger match ended in a draw reads `Draw`, because the simulation
/// records who has fallen and not when.
#[must_use]
pub fn viewer_result(
    result: MatchResult,
    players: &[PlayerStatusView; MAX_PLAYERS],
    viewer: PlayerId,
) -> Option<ViewerResult> {
    match result {
        MatchResult::InProgress => viewer
            .slot()
            .is_some_and(|slot| players[slot.index()].defeated)
            .then_some(ViewerResult::Eliminated),
        MatchResult::Victory { winner, .. } if winner == viewer => Some(ViewerResult::Victory),
        MatchResult::Victory { .. } => Some(ViewerResult::Defeat),
        MatchResult::Draw { .. } => Some(ViewerResult::Draw),
    }
}

/// The tick `result` ended on, or `None` while the match is in progress.
const fn final_tick(result: MatchResult) -> Option<Tick> {
    match result {
        MatchResult::InProgress => None,
        MatchResult::Victory { tick, .. } | MatchResult::Draw { tick } => Some(tick),
    }
}

/// The verdict on screen, as of the latest refresh.
#[derive(Resource, Debug, Default)]
pub struct ResultScreen {
    verdict: Option<ViewerResult>,
    /// Tick `length_line` was written for.
    written: Option<Tick>,
    length_line: String,
}

impl ResultScreen {
    /// What the screen is showing the viewer, if anything.
    #[must_use]
    pub const fn verdict(&self) -> Option<ViewerResult> {
        self.verdict
    }

    /// The cached match length. Empty while a match goes on, which an eliminated viewer sees.
    #[must_use]
    pub fn length_line(&self) -> &str {
        &self.length_line
    }

    /// The cached text buffer, as `(name, address, capacity)`.
    ///
    /// Evidence for the steady-state tests that a refresh reuses storage. Not part of the HUD's
    /// interface.
    #[doc(hidden)]
    pub fn retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)> + '_ {
        std::iter::once((
            "result.length_line",
            self.length_line.as_ptr().addr(),
            self.length_line.capacity(),
        ))
    }

    /// Brings the screen up to date, rewriting the text only when the final tick changed.
    fn update(
        &mut self,
        result: MatchResult,
        players: &[PlayerStatusView; MAX_PLAYERS],
        viewer: PlayerId,
    ) {
        self.verdict = viewer_result(result, players, viewer);
        let tick = final_tick(result);
        if self.written == tick {
            return;
        }
        self.written = tick;
        self.length_line.clear();
        if let Some(tick) = tick {
            self.length_line.push_str("Match length ");
            write_clock(&mut self.length_line, clock_seconds(tick));
        }
    }

    /// Shows nothing, for when there is no viewer.
    fn clear(&mut self) {
        self.verdict = None;
        self.written = None;
        self.length_line.clear();
    }
}

/// Brings [`ResultScreen`] up to date with the presented snapshot.
///
/// Constant time, with no allocation unless the match has just ended.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn refresh_result_screen(
    session: Res<'_, BattleSession>,
    model: Res<'_, HudModel>,
    mut screen: ResMut<'_, ResultScreen>,
) {
    let Some(viewer) = model.viewer else {
        screen.clear();
        return;
    };
    let presented = session.frames.presented();
    screen.update(presented.match_result, &presented.players, viewer);
}

/// What the player pressed on the result window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultChoice {
    PlayAgain,
    Exit,
}

/// Draws the result window, if there is a verdict and no failure to show instead, and acts on it.
///
/// Runs after [`battle_hud_system`](super::panels::battle_hud_system), so [`HudGeometry::failure`]
/// says whether a banner was drawn on this frame.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn battle_result_system(
    mut contexts: EguiContexts<'_, '_>,
    screen: Res<'_, ResultScreen>,
    mut lab: ResMut<'_, LabState>,
    mut geometry: ResMut<'_, HudGeometry>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let (Some(verdict), None) = (screen.verdict(), geometry.failure) else {
        return;
    };
    // `lab_ui_system` reports a live window without a context.
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    match draw_result_window(ctx, verdict, screen.length_line(), &mut geometry) {
        Some(ResultChoice::PlayAgain) => lab.battle_reset_requested = true,
        Some(ResultChoice::Exit) => {
            exit.send(AppExit::Success);
        }
        None => {}
    }
}

/// Draws the window in the middle of the world rectangle, and returns what was pressed.
fn draw_result_window(
    ctx: &egui::Context,
    verdict: ViewerResult,
    length_line: &str,
    geometry: &mut HudGeometry,
) -> Option<ResultChoice> {
    let mut choice = None;
    let window = egui::Window::new("battle_result")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .movable(false)
        .default_width(RESULT_WINDOW_WIDTH)
        .constrain_to(ctx.available_rect())
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(verdict.title());
                if !length_line.is_empty() {
                    ui.label(length_line);
                }
                let again = ui.button("Play again");
                geometry.play_again = visible(&again, ui.clip_rect());
                if again.clicked() {
                    choice = Some(ResultChoice::PlayAgain);
                }
                let exit = ui.button("Exit");
                geometry.exit = visible(&exit, ui.clip_rect());
                if exit.clicked() {
                    choice = Some(ResultChoice::Exit);
                }
            });
        });
    geometry.result = window.map(|shown| shown.response.rect);
    choice
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWER: PlayerId = PlayerId(2);
    const OTHER: PlayerId = PlayerId(5);
    const END: Tick = Tick(1_525);

    fn players(defeated: &[PlayerId]) -> [PlayerStatusView; 8] {
        let mut players = [PlayerStatusView::default(); 8];
        for player in defeated {
            players[usize::from(player.0)].defeated = true;
        }
        players
    }

    #[test]
    fn every_standing_has_one_meaning_for_the_viewer() {
        let won = MatchResult::Victory {
            winner: VIEWER,
            tick: END,
        };
        let lost = MatchResult::Victory {
            winner: OTHER,
            tick: END,
        };
        let draw = MatchResult::Draw { tick: END };
        let progress = MatchResult::InProgress;
        let table = [
            (progress, vec![], None),
            (progress, vec![OTHER], None),
            (progress, vec![VIEWER], Some(ViewerResult::Eliminated)),
            (won, vec![OTHER], Some(ViewerResult::Victory)),
            (lost, vec![VIEWER], Some(ViewerResult::Defeat)),
            // A viewer eliminated earlier learns the end as any other loser does.
            (lost, vec![], Some(ViewerResult::Defeat)),
            (draw, vec![VIEWER, OTHER], Some(ViewerResult::Draw)),
            (draw, vec![], Some(ViewerResult::Draw)),
        ];
        for (result, defeated, expected) in table {
            assert_eq!(
                viewer_result(result, &players(&defeated), VIEWER),
                expected,
                "{result:?} with {defeated:?} defeated"
            );
        }
    }

    #[test]
    fn a_viewer_outside_the_slots_is_never_eliminated() {
        let everyone = [PlayerStatusView {
            defeated: true,
            ..PlayerStatusView::default()
        }; 8];
        assert_eq!(
            viewer_result(MatchResult::InProgress, &everyone, PlayerId(8)),
            None
        );
    }

    #[test]
    fn the_verdict_reads_no_other_players_slot() {
        let mut others = [PlayerStatusView {
            minerals: 999,
            gas: 999,
            supply_used: 9,
            supply_max: 9,
            defeated: true,
        }; 8];
        others[usize::from(VIEWER.0)] = PlayerStatusView::default();
        let plain = players(&[]);
        for result in [
            MatchResult::InProgress,
            MatchResult::Draw { tick: END },
            MatchResult::Victory {
                winner: OTHER,
                tick: END,
            },
        ] {
            assert_eq!(
                viewer_result(result, &others, VIEWER),
                viewer_result(result, &plain, VIEWER)
            );
        }
    }

    #[test]
    fn the_title_of_every_result_is_its_word() {
        for (result, title) in [
            (ViewerResult::Victory, "Victory"),
            (ViewerResult::Defeat, "Defeat"),
            (ViewerResult::Draw, "Draw"),
            (ViewerResult::Eliminated, "Eliminated"),
        ] {
            assert_eq!(result.title(), title);
        }
    }

    #[test]
    fn the_length_is_the_final_tick_at_the_simulation_rate() {
        let mut screen = ResultScreen::default();
        screen.update(MatchResult::Draw { tick: END }, &players(&[]), VIEWER);
        assert_eq!(screen.verdict(), Some(ViewerResult::Draw));
        // 1525 ticks of 40 ms are 61 seconds.
        assert_eq!(screen.length_line(), "Match length 1:01");
    }

    #[test]
    fn an_eliminated_viewer_sees_no_length_until_the_match_ends() {
        let mut screen = ResultScreen::default();
        let fallen = players(&[VIEWER]);
        screen.update(MatchResult::InProgress, &fallen, VIEWER);
        assert_eq!(screen.verdict(), Some(ViewerResult::Eliminated));
        assert_eq!(screen.length_line(), "");

        let end = MatchResult::Victory {
            winner: OTHER,
            tick: END,
        };
        screen.update(end, &fallen, VIEWER);
        assert_eq!(screen.verdict(), Some(ViewerResult::Defeat));
        assert_eq!(screen.length_line(), "Match length 1:01");
    }

    #[test]
    fn an_unchanged_verdict_keeps_its_storage() {
        let mut screen = ResultScreen::default();
        let end = MatchResult::Draw { tick: END };
        screen.update(end, &players(&[]), VIEWER);
        let storage = (screen.length_line.as_ptr(), screen.length_line.capacity());
        for _ in 0..100 {
            screen.update(end, &players(&[]), VIEWER);
        }
        assert_eq!(
            (screen.length_line.as_ptr(), screen.length_line.capacity()),
            storage
        );
    }

    #[test]
    fn a_new_match_in_progress_shows_nothing() {
        let mut screen = ResultScreen::default();
        screen.update(MatchResult::Draw { tick: END }, &players(&[]), VIEWER);
        screen.update(MatchResult::InProgress, &players(&[]), VIEWER);
        assert_eq!(screen.verdict(), None);
        assert_eq!(screen.length_line(), "");
    }

    #[test]
    fn clearing_forgets_the_verdict_and_its_text() {
        let mut screen = ResultScreen::default();
        screen.update(MatchResult::Draw { tick: END }, &players(&[]), VIEWER);
        screen.clear();
        assert_eq!(screen.verdict(), None);
        assert_eq!(screen.length_line(), "");
        screen.update(MatchResult::Draw { tick: END }, &players(&[]), VIEWER);
        assert_eq!(screen.length_line(), "Match length 1:01");
    }
}
