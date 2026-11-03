//! The bounded log of messages shown to the player.
//!
//! # Responsibility
//! Turns refusals and task reports from the battle pipeline into a few short-lived lines of text.
//! Ordinary rejections never stop anything: this is how a player learns of them.
//!
//! # How it fits
//! [`collect_battle_feed`] reads, for the viewer only:
//! - the queue's immediate refusals, which happen on paused frames too;
//! - [`BattleSession::rejected`], the orders that failed revalidation on their tick, including
//!   every order still pending when the match ended;
//! - [`BattleSession::notices`] carrying a simulation refusal or a refund;
//! - [`BattleSession::task_events`], how each of the viewer's unit tasks ended.
//!
//! The wording comes from [`status`], so the feed and the inspector say the same thing.
//!
//! # Invariants
//! - At most [`FEED_LINES`] lines exist, and a new line replaces the oldest.
//! - Each piece of evidence is reported once. The session lists hold the evidence of the latest
//!   ticks, so they are read only when the tick has changed.
//! - A run of equal refusals from one tick's revalidation is one line, so the end of a match does
//!   not flood the feed. Separate refusals of the player's own gestures stay separate lines.
//! - Nothing of another player reaches a line.
//! - Accepted orders are not reported: a working order needs no message.

use bevy::prelude::*;
use sim_core::economy::{GameCommandReceipt, TaskEventKind};
use sim_types::PlayerId;

use super::model::HudModel;
use crate::battle_pipeline::driver::AdmissionOutcome;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_pipeline::status;

/// Most lines shown at once.
pub const FEED_LINES: usize = 5;

/// How long a line stays, in seconds of application time.
pub const FEED_TTL_SECONDS: f32 = 8.0;

/// What kind of news a line carries, which decides its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeedTone {
    /// Something the player asked for was refused.
    #[default]
    Refusal,
    /// Something happened that the player should know about.
    Notice,
    /// Something finished well.
    Milestone,
}

/// One line of the feed.
#[derive(Debug, Default)]
pub struct FeedLine {
    /// The message.
    pub text: String,
    /// Its kind.
    pub tone: FeedTone,
    /// Application time at which the line disappears, in seconds.
    expires_at: f32,
}

// PERFORMANCE RATIONALE:
// Why: A line is written from buffered evidence that can repeat throughout a battle.
// What: A fixed ring of strings, each cleared and rewritten in place.
// Trade-off: A hard limit of `FEED_LINES`, which a five-line feed wants anyway.
/// Bounded message log. Reset for each session.
#[derive(Resource, Debug, Default)]
pub struct BattleFeed {
    lines: [FeedLine; FEED_LINES],
    oldest: usize,
    len: usize,
    /// Tick of the latest evidence read from the session.
    seen_tick: Option<u32>,
    /// Serial of the latest queue refusal read.
    seen_refusal: u64,
}

impl BattleFeed {
    /// The live lines, oldest first.
    pub fn lines(&self) -> impl Iterator<Item = &FeedLine> {
        (0..self.len).map(|offset| &self.lines[(self.oldest + offset) % FEED_LINES])
    }

    /// Removes every line and forgets what was read, for a new session.
    pub fn clear(&mut self) {
        self.oldest = 0;
        self.len = 0;
        self.seen_tick = None;
        self.seen_refusal = 0;
    }

    /// Removes the lines whose time has passed, at application time `now`, in seconds.
    ///
    /// Every line lives equally long, so the oldest always goes first.
    pub fn expire(&mut self, now: f32) {
        while self.len > 0 && self.lines[self.oldest].expires_at <= now {
            self.oldest = (self.oldest + 1) % FEED_LINES;
            self.len -= 1;
        }
    }

    /// Starts a new line at application time `now`, replacing the oldest if the feed is full,
    /// and returns its emptied text for a writer to append to.
    pub fn begin_line(&mut self, tone: FeedTone, now: f32) -> &mut String {
        let index = (self.oldest + self.len) % FEED_LINES;
        if self.len == FEED_LINES {
            self.oldest = (self.oldest + 1) % FEED_LINES;
        } else {
            self.len += 1;
        }
        let line = &mut self.lines[index];
        line.tone = tone;
        line.expires_at = now + FEED_TTL_SECONDS;
        line.text.clear();
        &mut line.text
    }

    /// Reads whatever `session` has newly reported to `viewer`.
    fn ingest(&mut self, session: &BattleSession, viewer: PlayerId, now: f32) {
        if let Some((serial, refusal)) = session.commands.latest_refusal() {
            if serial != self.seen_refusal {
                self.seen_refusal = serial;
                let line = self.begin_line(FeedTone::Refusal, now);
                status::write_submission_rejection(line, refusal);
            }
        }
        let tick = session.published().map(|published| published.tick);
        if tick == self.seen_tick {
            return;
        }
        self.seen_tick = tick;
        let mut previous = None;
        for rejected in session.rejected.iter().filter(|r| r.player == viewer) {
            if previous != Some(&rejected.reason) {
                let line = self.begin_line(FeedTone::Refusal, now);
                status::write_submission_rejection(line, &rejected.reason);
            }
            previous = Some(&rejected.reason);
        }
        for notice in session
            .notices
            .iter()
            .filter(|notice| notice.outcome.player() == viewer)
        {
            if let Some(tone) = outcome_tone(&notice.outcome) {
                status::write_outcome(self.begin_line(tone, now), &notice.outcome);
            }
        }
        for event in session.task_events.iter().filter(|e| e.owner == viewer) {
            let line = self.begin_line(task_tone(&event.kind), now);
            status::write_task_event(line, &event.kind);
            capitalize(line);
        }
    }
}

/// How an outcome is reported, or `None` if a player need not hear of it.
fn outcome_tone(outcome: &AdmissionOutcome) -> Option<FeedTone> {
    let refused = match outcome {
        AdmissionOutcome::Movement(outcome) => outcome.result.is_err(),
        AdmissionOutcome::Production(outcome) => outcome.result.is_err(),
        AdmissionOutcome::Game(outcome) => match outcome.result {
            Err(_) => true,
            Ok(GameCommandReceipt::Refunded { .. }) => return Some(FeedTone::Notice),
            Ok(GameCommandReceipt::Accepted { .. }) => false,
        },
    };
    refused.then_some(FeedTone::Refusal)
}

const fn task_tone(kind: &TaskEventKind) -> FeedTone {
    match kind {
        TaskEventKind::StructureCompleted { .. } => FeedTone::Milestone,
        _ => FeedTone::Notice,
    }
}

/// Raises the first letter of a sentence. Every status sentence starts with an ASCII letter.
fn capitalize(line: &mut str) {
    if let Some(first) = line.get_mut(..1) {
        first.make_ascii_uppercase();
    }
}

/// Removes expired lines and appends what the session newly reported to the viewer.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn collect_battle_feed(
    time: Res<'_, Time>,
    session: Res<'_, BattleSession>,
    model: Res<'_, HudModel>,
    mut feed: ResMut<'_, BattleFeed>,
) {
    let now = time.elapsed_secs();
    feed.expire(now);
    if let Some(viewer) = model.viewer {
        feed.ingest(&session, viewer, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(feed: &BattleFeed) -> Vec<&str> {
        feed.lines().map(|line| line.text.as_str()).collect()
    }

    fn push(feed: &mut BattleFeed, text: &str, now: f32) {
        feed.begin_line(FeedTone::Notice, now).push_str(text);
    }

    #[test]
    fn a_full_feed_replaces_its_oldest_line() {
        let mut feed = BattleFeed::default();
        for index in 0..=FEED_LINES {
            push(&mut feed, &index.to_string(), 0.0);
        }
        assert_eq!(texts(&feed), ["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn lines_expire_oldest_first_at_their_time() {
        let mut feed = BattleFeed::default();
        push(&mut feed, "first", 0.0);
        push(&mut feed, "second", 1.0);
        feed.expire(FEED_TTL_SECONDS - 0.5);
        assert_eq!(texts(&feed), ["first", "second"]);
        feed.expire(FEED_TTL_SECONDS);
        assert_eq!(texts(&feed), ["second"]);
        feed.expire(FEED_TTL_SECONDS + 1.0);
        assert_eq!(texts(&feed).len(), 0);
    }

    #[test]
    fn the_ring_survives_wrapping_after_expiry() {
        let mut feed = BattleFeed::default();
        for index in 0..4 {
            push(&mut feed, &format!("old {index}"), 0.0);
        }
        feed.expire(FEED_TTL_SECONDS);
        for index in 0..FEED_LINES {
            push(&mut feed, &format!("new {index}"), 10.0);
        }
        assert_eq!(texts(&feed), ["new 0", "new 1", "new 2", "new 3", "new 4"]);
    }

    #[test]
    fn a_rewritten_line_keeps_its_storage() {
        let mut feed = BattleFeed::default();
        for _ in 0..FEED_LINES {
            feed.begin_line(FeedTone::Notice, 0.0)
                .push_str("long enough to have needed its own allocation");
        }
        let storage: Vec<_> = feed.lines().map(|line| line.text.as_ptr()).collect();
        for _ in 0..FEED_LINES {
            feed.begin_line(FeedTone::Refusal, 1.0).push_str("short");
        }
        let reused: Vec<_> = feed.lines().map(|line| line.text.as_ptr()).collect();
        assert_eq!(storage, reused);
    }

    #[test]
    fn clearing_forgets_lines_and_what_was_read() {
        let mut feed = BattleFeed::default();
        push(&mut feed, "stale", 0.0);
        feed.seen_tick = Some(9);
        feed.seen_refusal = 3;
        feed.clear();
        assert_eq!(texts(&feed).len(), 0);
        assert_eq!(feed.seen_tick, None);
        assert_eq!(feed.seen_refusal, 0);
    }

    #[test]
    fn a_sentence_is_capitalized_only_at_its_first_letter() {
        let mut line = String::from("a unit could not reach its task");
        capitalize(&mut line);
        assert_eq!(line, "A unit could not reach its task");
        let mut empty = String::new();
        capitalize(&mut empty);
        assert_eq!(empty, "");
    }

    #[test]
    fn only_a_completion_is_a_milestone() {
        assert_eq!(task_tone(&TaskEventKind::Unreachable), FeedTone::Notice);
        assert_eq!(
            task_tone(&TaskEventKind::StructureCompleted {
                foundation: sim_types::EntityId(1),
            }),
            FeedTone::Milestone
        );
    }
}
