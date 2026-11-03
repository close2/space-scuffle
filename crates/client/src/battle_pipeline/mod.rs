//! Owns queued battle intent, the single deterministic tick driver and retained
//! presentation snapshots. Session commits invalidate old generations atomically;
//! rendering never advances the simulation or consumes untracked events.

pub mod clock;
pub mod driver;
pub mod fog;
pub mod session;
pub mod status;

pub mod commands;
pub mod timeline;

/// Ordered live-client stages, shared by normal startup and deterministic captures.
#[derive(bevy::prelude::SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleSet {
    /// External capture/controller intent precedes ordinary UI collection.
    ExternalControl,
    /// UI observes the last fully published model and queues new intent.
    CollectIntent,
    /// Camera/world replacement commits before the explicit deferred boundary.
    CommitSession,
    /// The sole driver consumes commands and executes actual simulation ticks.
    AdvanceTicks,
    /// Cached models and scene membership consume the newly published state.
    PreparePresentation,
    /// Pose consumers and VFX observe matching published records and transforms.
    Present,
}
