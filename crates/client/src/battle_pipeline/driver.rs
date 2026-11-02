//! Sole live-client tick execution and immediate command-outcome handoff. Every
//! executed tick publishes its snapshot and drains outcomes before another tick
//! may clear them; presentation failure never erases already completed evidence.

use std::fmt::Write;
use std::time::Duration;

use bevy::prelude::*;
use sim_core::{movement::MovementOutcome, production::ProductionOutcome, SimWorld};
use sim_types::Tick;

use super::clock::BattleClockError;
use super::commands::{SessionGeneration, SubmissionError, SubmissionSequence};
use super::session::BattleSession;
use super::timeline::TimelineError;

/// Original authoritative admission result with local request correlation.
#[derive(Debug, Clone)]
pub enum AdmissionOutcome {
    /// One actual Produce transaction result.
    Production(ProductionOutcome),
    /// One actual Move/AttackMove transaction result.
    Movement(MovementOutcome),
}

impl AdmissionOutcome {
    fn identity(&self) -> (Tick, usize, sim_types::PlayerId) {
        match self {
            Self::Production(value) => (value.tick, value.command_index, value.player),
            Self::Movement(value) => (value.tick, value.command_index, value.player),
        }
    }
}

/// Authoritative outcome tied to the exact local submission and committed world.
#[derive(Debug, Clone)]
pub struct CommandNotice {
    /// Successful scene generation, never inferred from current selection.
    pub generation: SessionGeneration,
    /// Local command identity aligned with the actual input-vector index.
    pub sequence: SubmissionSequence,
    /// Unmodified simulation receipt/rejection evidence.
    pub outcome: AdmissionOutcome,
}

/// Fatal pipeline failures; ordinary command rejections remain separate evidence.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// Render ownership does not resolve to the loaded authored palette.
    #[error(transparent)]
    Presentation(#[from] crate::player_presentation::PresentationError),
    /// Invalid scheduling input or exhausted diagnostic duration.
    #[error(transparent)]
    Clock(#[from] BattleClockError),
    /// Invalid publication lifecycle or exhausted tick identity.
    #[error(transparent)]
    Timeline(#[from] TimelineError),
    /// Fatal intent/session preparation failure.
    #[error(transparent)]
    Submission(#[from] SubmissionError),
    /// Loaded candidate metadata failed validation before commit.
    #[error("battle metadata invariant: {0}")]
    Metadata(&'static str),
    /// World and retained output must identify the same pre-step boundary.
    #[error("battle world tick {world} differs from published tick {snapshot}")]
    Boundary {
        /// Current authoritative world tick.
        world: u32,
        /// Latest published authoritative snapshot tick.
        snapshot: u32,
    },
    /// Outcome identity did not refer to the actual executed command.
    #[error("battle outcome does not match its executed command at tick {tick}, index {index}")]
    Outcome {
        /// Actual outcome tick.
        tick: u32,
        /// Actual command index.
        index: usize,
    },
    /// Persistent evidence counters may not wrap.
    #[error("battle command outcome counter exhausted")]
    OutcomeCount,
    /// An actual simulation phase reported a fatal invariant.
    #[error("simulation failed: {0}")]
    Simulation(String),
    /// The completed snapshot cannot be represented by its exact media.
    #[error(transparent)]
    Media(#[from] crate::media::ClientMediaError),
}

impl BattleSession {
    /// Executes bounded eligible ticks and publishes a pose even on no-tick frames.
    /// `validate_media` inspects actual initial and per-tick output before rendering.
    /// # Errors
    /// Stops on typed failure while retaining every already executed tick's evidence.
    pub fn advance(
        &mut self,
        world: &mut SimWorld,
        elapsed: Duration,
        active: bool,
        paused: bool,
        speed: u32,
        mut validate_media: impl FnMut(
            &sim_types::RenderSnapshot,
        ) -> Result<(), crate::media::ClientMediaError>,
    ) -> Result<(), PipelineError> {
        self.notices.clear();
        self.rejected.clear();
        self.diagnostics.clear();
        self.unmatched_outcomes.clear();
        if !self.ready {
            self.validate_output(&mut validate_media)?;
            self.ready = true;
        }
        if !active || paused || speed == 0 {
            self.suspend();
            self.frames.present(1.0)?;
            return Ok(());
        }
        let scheduled = self.clock.observe(elapsed, speed)?;
        for _ in 0..scheduled.ticks {
            if world.current_tick != self.frames.current().tick.0 {
                return Err(PipelineError::Boundary {
                    world: world.current_tick,
                    snapshot: self.frames.current().tick.0,
                });
            }
            // Identity preflight precedes dequeue so exhaustion preserves pending intent.
            self.frames.preflight_next_tick()?;
            self.commands.prepare_tick(
                world,
                &mut self.input,
                &mut self.sequences,
                &mut self.rejected,
            )?;
            world.step_into(&self.input, self.frames.begin_tick()?);
            let publication = self.frames.finish_tick();
            let outcomes = self.capture_outcomes(world);
            self.clock.consume_tick()?;
            publication?;
            outcomes?;
            self.diagnostics.extend(world.drain_diagnostics());
            let diagnostic = self.diagnostics.first().map(ToString::to_string);
            for diagnostic in &self.diagnostics {
                error!("Simulation failed: {diagnostic}");
            }
            self.validate_output(&mut validate_media)?;
            if let Some(diagnostic) = diagnostic {
                return Err(PipelineError::Simulation(diagnostic));
            }
        }
        self.frames.present(self.clock.fraction())?;
        Ok(())
    }

    fn validate_output(
        &mut self,
        check: &mut impl FnMut(&sim_types::RenderSnapshot) -> Result<(), crate::media::ClientMediaError>,
    ) -> Result<(), PipelineError> {
        if let Err(error) = check(self.frames.current()) {
            self.presentation_blocked = true;
            return Err(error.into());
        }
        Ok(())
    }

    fn record_outcome(
        &mut self,
        generation: SessionGeneration,
        outcome: AdmissionOutcome,
    ) -> Result<(), PipelineError> {
        let (tick, index, player) = outcome.identity();
        let matched = self.sequences.get(index).copied().filter(|_| {
            self.input
                .commands
                .get(index)
                .is_some_and(|command| command.player_id == player)
                && tick == self.frames.current().tick
        });
        let Some(sequence) = matched else {
            self.unmatched_outcomes.push(outcome);
            return Err(PipelineError::Outcome {
                tick: tick.0,
                index,
            });
        };
        self.notices.push(CommandNotice {
            generation,
            sequence,
            outcome,
        });
        Ok(())
    }

    fn capture_outcomes(&mut self, world: &mut SimWorld) -> Result<(), PipelineError> {
        let generation = self
            .commands
            .generation()
            .ok_or(SubmissionError::NotReady)?;
        let start = self.notices.len();
        let mut failure = None;
        for outcome in world.drain_production_outcomes() {
            if let Err(error) =
                self.record_outcome(generation, AdmissionOutcome::Production(outcome))
            {
                failure = Some(error);
            }
        }
        for outcome in world.drain_movement_outcomes() {
            if let Err(error) = self.record_outcome(generation, AdmissionOutcome::Movement(outcome))
            {
                failure = Some(error);
            }
        }
        self.notices[start..].sort_unstable_by_key(|notice| notice.outcome.identity().1);
        let added =
            u64::try_from(self.notices.len() - start).map_err(|_| PipelineError::OutcomeCount)?;
        self.outcome_count = self
            .outcome_count
            .checked_add(added)
            .ok_or(PipelineError::OutcomeCount)?;
        if let Some(last) = self.notices.last().filter(|_| added > 0) {
            self.outcome_status.clear();
            super::status::write_outcome(&mut self.outcome_status, &last.outcome);
        }
        failure.map_or(Ok(()), Err)
    }
}

/// Executes the only production client simulation tick path.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn advance_battle_ticks(
    time: Res<'_, Time>,
    mut lab: ResMut<'_, crate::LabState>,
    mut session: ResMut<'_, BattleSession>,
    registry: Res<'_, crate::FlipbookRegistry>,
    mut media: ResMut<'_, crate::media::ClientMediaStatus>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if media.retain_failure(&mut lab) || lab.battle_failure.is_some() {
        session.suspend();
        return;
    }
    let active = lab.current_tab == crate::LabTab::LiveBattle;
    let paused = lab.battle_paused;
    let speed = lab.simulation_speed;
    if session.world().is_none() {
        return;
    }
    let before = session.frames.current().tick;
    let result = session.advance_committed(time.delta(), active, paused, speed, |snapshot| {
        for instance in &snapshot.bubbles {
            media.require(&registry, instance.unit_type, instance.anim_state)?;
        }
        Ok(())
    });
    if session.frames.current().tick != before || lab.last_checksum_hex.is_empty() {
        lab.last_checksum_hex.clear();
        let _ = write!(
            lab.last_checksum_hex,
            "0x{:016X}",
            session.frames.current().checksum
        );
    }
    if let Err(error) = result {
        session.suspend();
        if !session.presentation_blocked {
            if let Err(publication) = session.frames.present(1.0) {
                error!("Failure pose publication rejected: {publication}");
            }
        }
        match error {
            PipelineError::Media(error) => {
                crate::media::reject(error, &mut media, &mut lab, &mut exit);
            }
            other => {
                error!("Battle pipeline failed: {other}");
                lab.battle_failure = Some(other.to_string());
                lab.battle_paused = true;
                exit.send(AppExit::error());
            }
        }
    }
}

/// Delivers only events whose corresponding pose has already been published.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn publish_combat_events(
    lab: Res<'_, crate::LabState>,
    mut session: ResMut<'_, BattleSession>,
    mut events: EventWriter<'_, crate::CombatVisualEvent>,
) {
    if lab.current_tab != crate::LabTab::LiveBattle || session.presentation_blocked {
        return;
    }
    while let Some(event) = session.frames.pop_presented_event() {
        events.send(crate::CombatVisualEvent(event.event));
    }
}

#[cfg(test)]
mod tests;
