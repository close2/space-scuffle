//! Retains adjacent immutable snapshots and their ordered combat evidence. Spatial
//! presentation advances every frame; discrete membership and events advance only
//! at the corresponding displayed tick. Pause/tab return hold the exact current pose.

use std::collections::VecDeque;

use bevy::prelude::Resource;
use sim_types::{
    AreaView, BubbleRenderInstance, CombatEventVisual, LinkView, RenderSnapshot, Tick,
};

use super::commands::SessionGeneration;

/// One visual event's original authoritative boundary and stable within-tick order.
#[derive(Clone, Debug, PartialEq)]
pub struct PresentedCombatEvent {
    /// Successful session commit that produced this event.
    pub generation: SessionGeneration,
    /// Fully executed simulation tick at which the event was created.
    pub tick: Tick,
    /// Zero-based order inside that tick's event output.
    pub within_tick_sequence: usize,
    /// Immutable original event positions, unaffected by later entity reuse.
    pub event: CombatEventVisual,
}

/// Explicit misuse or unrepresentable timeline identity; no silent default pose.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimelineError {
    /// A successful session must publish its initial snapshot before ticking.
    #[error("battle presentation session is not initialized")]
    NotInitialized,
    /// Initial publication must complete exactly once after initialization starts.
    #[error("battle initial snapshot is not awaiting publication")]
    NoInitialization,
    /// Replacement identities must be newer than the retained session.
    #[error("battle snapshot generation is not newer than the retained session")]
    StaleGeneration,
    /// One output buffer is already awaiting publication.
    #[error("battle snapshot tick is already in progress")]
    TickInProgress,
    /// Completion must correspond to an explicitly prepared tick.
    #[error("battle snapshot completion has no prepared tick")]
    NoPreparedTick,
    /// Client presentation identities may not wrap to an older tick.
    #[error("battle simulation tick identity exhausted")]
    TickExhausted,
    /// The output must identify exactly the prepared next authoritative tick.
    #[error("battle snapshot tick {actual} differs from expected {expected}")]
    TickSequence {
        /// Sequential identity prepared before simulation execution.
        expected: u32,
        /// Identity actually written by the tick driver.
        actual: u32,
    },
    /// Interpolation must never publish a nonfinite or extrapolated position.
    #[error("battle interpolation requires a finite fraction from zero through one")]
    InvalidFraction,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum PublicationPhase {
    #[default]
    Uninitialized,
    Initializing,
    Ready,
    Tick {
        expected: u32,
    },
}

/// Retained output buffers and pending visual evidence for one committed session.
#[derive(Resource, Default)]
pub struct BattleFrameState {
    generation: Option<SessionGeneration>,
    previous: RenderSnapshot,
    current: RenderSnapshot,
    presented: RenderSnapshot,
    previous_valid: bool,
    phase: PublicationPhase,
    presented_valid: bool,
    /// Interpolation fraction of the presented pose towards the tick after `presented.tick`:
    /// the `alpha` of the last [`Self::present`], or 0 when the current tick is shown exactly.
    presented_fraction: f32,
    hold_current: bool,
    pending_events: VecDeque<PresentedCombatEvent>,
}

impl BattleFrameState {
    /// Resets retained buffers for a successfully committed generation.
    /// The caller fills the returned current snapshot once through the real world API.
    /// # Errors
    /// Rejects a reused or older generation before changing retained evidence.
    pub fn initialize(
        &mut self,
        generation: SessionGeneration,
    ) -> Result<&mut RenderSnapshot, TimelineError> {
        if self.generation.is_some_and(|current| generation <= current) {
            return Err(TimelineError::StaleGeneration);
        }
        self.generation = Some(generation);
        self.previous.bubbles.clear();
        self.previous.combat_events.clear();
        self.current.bubbles.clear();
        self.current.combat_events.clear();
        self.presented.bubbles.clear();
        self.presented.combat_events.clear();
        self.previous_valid = false;
        self.phase = PublicationPhase::Initializing;
        self.presented_valid = false;
        self.presented_fraction = 0.0;
        self.current.tick = Tick(0);
        self.current.checksum = 0;
        self.hold_current = true;
        self.pending_events.clear();
        Ok(&mut self.current)
    }

    /// Returns the session whose snapshots are initialized.
    #[must_use]
    pub fn generation(&self) -> Option<SessionGeneration> {
        self.generation
    }

    /// Recycles the older buffer as output for exactly one new simulation tick.
    /// Call `finish_tick` afterward before preparing another tick or presentation.
    /// # Errors
    /// Rejects missing initialization, unfinished output or exhausted tick identity.
    pub fn begin_tick(&mut self) -> Result<&mut RenderSnapshot, TimelineError> {
        let next = self.preflight_next_tick()?;
        self.phase = PublicationPhase::Tick { expected: next };
        std::mem::swap(&mut self.previous, &mut self.current);
        self.previous_valid = true;
        Ok(&mut self.current)
    }

    /// Checks the next monotonic identity without rotating buffers or consuming intent.
    /// # Errors
    /// Rejects unpublished state or exhausted identity before command preparation.
    pub fn preflight_next_tick(&self) -> Result<u32, TimelineError> {
        self.require_published()?;
        self.current
            .tick
            .0
            .checked_add(1)
            .ok_or(TimelineError::TickExhausted)
    }

    /// Publishes a completed tick and retains every visual event before the next swap.
    /// # Errors
    /// Rejects missing preparation or a mismatched output tick before draining events.
    pub fn finish_tick(&mut self) -> Result<(), TimelineError> {
        let PublicationPhase::Tick { expected } = self.phase else {
            return Err(TimelineError::NoPreparedTick);
        };
        if self.current.tick.0 != expected {
            return Err(TimelineError::TickSequence {
                expected,
                actual: self.current.tick.0,
            });
        }
        self.stage_events()?;
        self.phase = PublicationPhase::Ready;
        self.hold_current = false;
        Ok(())
    }

    /// Stages initial-world events without creating a synthetic prior snapshot.
    /// # Errors
    /// Rejects an absent committed identity or an unfinished later tick.
    pub fn finish_initialization(&mut self) -> Result<(), TimelineError> {
        if matches!(self.phase, PublicationPhase::Tick { .. }) {
            return Err(TimelineError::TickInProgress);
        }
        if self.phase != PublicationPhase::Initializing {
            return Err(TimelineError::NoInitialization);
        }
        self.stage_events()?;
        self.phase = PublicationPhase::Ready;
        Ok(())
    }

    fn require_published(&self) -> Result<(), TimelineError> {
        match self.phase {
            PublicationPhase::Uninitialized | PublicationPhase::Initializing => {
                Err(TimelineError::NotInitialized)
            }
            PublicationPhase::Tick { .. } => Err(TimelineError::TickInProgress),
            PublicationPhase::Ready => Ok(()),
        }
    }

    fn stage_events(&mut self) -> Result<(), TimelineError> {
        let generation = self.generation.ok_or(TimelineError::NotInitialized)?;
        for (within_tick_sequence, event) in self.current.combat_events.drain(..).enumerate() {
            self.pending_events.push_back(PresentedCombatEvent {
                generation,
                tick: self.current.tick,
                within_tick_sequence,
                event,
            });
        }
        Ok(())
    }

    /// Snaps paused/suspended presentation to current and prevents resume rewind.
    pub fn hold_exact(&mut self) {
        self.hold_current = true;
    }

    /// Returns the latest authoritative snapshot, without consuming its event bridge.
    #[must_use]
    pub fn current(&self) -> &RenderSnapshot {
        &self.current
    }

    /// Returns the previous complete authoritative boundary when an adjacent pair exists.
    /// This read-only view is intended for presentation verification and diagnostics.
    #[must_use]
    pub fn previous(&self) -> Option<&RenderSnapshot> {
        self.previous_valid.then_some(&self.previous)
    }

    /// Returns the reused displayed snapshot; it contains no unconsumed combat events.
    #[must_use]
    pub fn presented(&self) -> &RenderSnapshot {
        &self.presented
    }

    /// The persistent areas and tethers of the presented tick (plan
    /// `2026-11-09-effect-visualization` WP10): those of the previous snapshot while the pose
    /// is interpolated from it, else those of the current one. Like the bubbles' membership,
    /// they change only at the displayed tick; read in place, never copied.
    #[must_use]
    pub fn presented_effect_views(&self) -> (&[AreaView], &[LinkView]) {
        let source = if self.presented_valid
            && self.previous_valid
            && self.presented.tick == self.previous.tick
            && self.presented.tick != self.current.tick
        {
            &self.previous
        } else {
            &self.current
        };
        (&source.areas, &source.links)
    }

    /// Computes a frame's pose into reusable storage; alpha is a finite fraction from zero through one.
    /// Discrete records and membership remain at previous until the current tick is reached.
    /// # Errors
    /// Rejects nonfinite/out-of-range fractions, absent sessions or unpublished ticks.
    pub fn present(&mut self, alpha: f32) -> Result<(), TimelineError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(TimelineError::InvalidFraction);
        }
        self.require_published()?;
        self.presented.bubbles.clear();
        self.presented.combat_events.clear();
        self.presented_valid = true;
        if self.hold_current || !self.previous_valid || alpha >= 1.0 {
            self.presented_fraction = 0.0;
            self.presented.tick = self.current.tick;
            self.presented.checksum = self.current.checksum;
            self.presented.copy_match_status_from(&self.current);
            self.presented
                .bubbles
                .extend(self.current.bubbles.iter().cloned());
            return Ok(());
        }
        self.presented_fraction = alpha;
        self.presented.tick = self.previous.tick;
        self.presented.checksum = self.previous.checksum;
        self.presented.copy_match_status_from(&self.previous);
        // PERFORMANCE RATIONALE:
        // Why: Membership matching must not scan all entities for every record.
        // What: Merge the two authoritative entity-ID-ordered sequences once.
        // Trade-off: Stable snapshot ordering is required; extraction guarantees it.
        let mut next = self.current.bubbles.iter().peekable();
        for previous in &self.previous.bubbles {
            while next
                .peek()
                .is_some_and(|current| current.entity_id < previous.entity_id)
            {
                next.next();
            }
            let mut record = previous.clone();
            if let Some(current) = next.peek().filter(|current| compatible(previous, current)) {
                interpolate(&mut record, current, alpha);
            }
            self.presented.bubbles.push(record);
        }
        Ok(())
    }

    /// The presented simulation instant: the presented record's tick and the fraction, in
    /// `[0, 1)`, of the way towards the next tick that positions were interpolated by.
    ///
    /// The flipbook clock is built from it (plan `2026-11-04-battle-flipbook-animation` D-1), so
    /// it advances with presented simulation time only: it stands still while paused and runs
    /// at the battle's speed. Before the first presentation it is `(Tick(0), 0.0)`.
    #[must_use]
    pub fn presented_instant(&self) -> (Tick, f32) {
        (self.presented.tick, self.presented_fraction)
    }

    /// Removes the next event eligible for the displayed tick, preserving original order.
    pub fn pop_presented_event(&mut self) -> Option<PresentedCombatEvent> {
        if self.presented_valid
            && self
                .pending_events
                .front()
                .is_some_and(|event| event.tick <= self.presented.tick)
        {
            self.pending_events.pop_front()
        } else {
            None
        }
    }

    /// The buffers rebuilt on every frame, as `(name, address, capacity in elements)`.
    ///
    /// `previous` and `current` trade places on each tick by design, so only the presented pose
    /// and the event queue are listed. A `VecDeque` exposes no buffer address, only a moving
    /// front, so the queue reports address 0 and its capacity. Evidence for the steady-state
    /// tests; not part of the timeline's interface.
    #[doc(hidden)]
    pub fn retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)> + '_ {
        [
            (
                "frames.presented.bubbles",
                self.presented.bubbles.as_ptr().addr(),
                self.presented.bubbles.capacity(),
            ),
            (
                "frames.presented.combat_events",
                self.presented.combat_events.as_ptr().addr(),
                self.presented.combat_events.capacity(),
            ),
            ("frames.pending_events", 0, self.pending_events.capacity()),
        ]
        .into_iter()
    }

    /// Returns retained events awaiting their displayed tick, for diagnostics/tests.
    #[must_use]
    pub fn pending_event_count(&self) -> usize {
        self.pending_events.len()
    }
}

fn compatible(previous: &BubbleRenderInstance, current: &BubbleRenderInstance) -> bool {
    previous.entity_id == current.entity_id
        && previous.unit_type == current.unit_type
        && previous.shape == current.shape
        && previous.owner == current.owner
        && previous.radius.to_bits() == current.radius.to_bits()
}

fn interpolate(record: &mut BubbleRenderInstance, current: &BubbleRenderInstance, alpha: f32) {
    for (previous, current) in record.world_pos.iter_mut().zip(current.world_pos) {
        *previous += (current - *previous) * alpha;
    }
    record.ground_z += (current.ground_z - record.ground_z) * alpha;
    for (previous, current) in record.velocity.iter_mut().zip(current.velocity) {
        *previous += (current - *previous) * alpha;
    }
    record.heading_angle_rad = angle(record.heading_angle_rad, current.heading_angle_rad, alpha);
    record.aim_angle_rad = angle(record.aim_angle_rad, current.aim_angle_rad, alpha);
}

fn angle(previous: f32, current: f32, alpha: f32) -> f32 {
    let difference = (current - previous + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    previous + difference * alpha
}

#[cfg(test)]
mod tests;
