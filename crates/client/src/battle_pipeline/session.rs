//! Owns one committed battle: its authoritative world and its command, timing and
//! presentation lifetime. Candidate
//! sessions are prepared before scene replacement; failed candidates leave the
//! existing generation, metadata, commands and published evidence intact.

use bevy::prelude::Resource;
use bevy_egui::egui;
use sim_core::SimWorld;
use sim_types::{PlayerId, Tick, TickBundle};

use super::clock::BattleTickClock;
use super::commands::{
    BattleCommandQueue, RejectedSubmission, SessionGeneration, SubmissionSequence,
};
use super::driver::{CommandNotice, PipelineError};
use super::timeline::BattleFrameState;

/// One validated scenario participant's immutable presentation identity.
#[derive(Clone, Debug)]
pub struct ParticipantMetadata {
    /// Actual simulation owner, without assumptions about contiguous slots.
    pub owner: PlayerId,
    /// Cached authored player/loadout label.
    pub label: String,
    /// Authored sRGB color, converted once for the inspector.
    pub color: egui::Color32,
}

/// Small metadata snapshot belonging to a successfully committed world.
#[derive(Clone, Debug, Default)]
pub struct CommittedBattleMetadata {
    /// Actual loaded map title.
    pub map_title: String,
    /// Validated participant labels, sorted by owner.
    pub players: Vec<ParticipantMetadata>,
}

impl CommittedBattleMetadata {
    /// Prepares labels from a candidate definition without changing the live world.
    /// # Errors
    /// Rejects missing loadouts or owner colors instead of relabeling a retained scene.
    pub fn from_definition(
        definition: &crate::runtime_battle::BattleDefinition,
    ) -> Result<Self, PipelineError> {
        let mut players = Vec::with_capacity(definition.scenario.players.len());
        for player in &definition.scenario.players {
            let loadout = definition
                .loadouts
                .get(&player.loadout_slug)
                .ok_or(PipelineError::Metadata("validated loadout is absent"))?;
            let style = definition.palette.resolve(PlayerId(player.player_slot))?;
            players.push(ParticipantMetadata {
                owner: PlayerId(player.player_slot),
                label: format!("Player {} · {}", player.player_slot, loadout.display_name),
                color: style.ui,
            });
        }
        players.sort_by_key(|player| player.owner);
        Ok(Self {
            map_title: definition.document.metadata.title.clone(),
            players,
        })
    }
}

/// Authoritative state copied for presentation at the end of each executed tick.
///
/// Presentation reads this record instead of the world, which keeps fixed-point simulation state
/// behind the session boundary.
///
// PERFORMANCE RATIONALE:
// Why: Map objects are read every frame but only change on a simulation tick.
// What: One record whose buffers are refilled in place when the tick advances.
// Trade-off: A small retained copy in exchange for no per-frame allocation or world access.
#[derive(Debug, Clone, Default)]
pub struct PublishedBattleState {
    /// Simulation tick these values describe.
    pub tick: u32,
    /// Number of entities in the authoritative world, alive or awaiting removal.
    pub entity_count: usize,
    /// Whether each map resource still has anything to harvest, in map resource order.
    pub resources_present: Vec<bool>,
    /// Controlling player of each watchtower, in map watchtower order.
    pub watchtower_owners: Vec<Option<PlayerId>>,
}

impl PublishedBattleState {
    fn refresh(&mut self, world: &SimWorld) {
        self.tick = world.current_tick;
        self.entity_count = world.entities.len();
        self.resources_present.clear();
        self.resources_present
            .extend(world.map.resources().iter().map(|node| node.remaining > 0));
        self.watchtower_owners.clear();
        self.watchtower_owners.extend(
            world
                .map
                .watchtowers()
                .iter()
                .map(|tower| tower.controlling_player),
        );
    }
}

/// Persistent storage for a single successful battle generation.
#[derive(Resource)]
pub struct BattleSession {
    /// Authoritative simulation of this generation; absent until a battle is committed.
    world: Option<SimWorld>,
    /// Presentation copy of `world`, present exactly when `world` is.
    published: Option<PublishedBattleState>,
    /// Captured intent; only the sole tick driver dequeues it.
    pub commands: BattleCommandQueue,
    /// Checked wall-time scheduling debt and slowdown diagnostics.
    pub clock: BattleTickClock,
    /// Adjacent authoritative snapshots and ordered visual evidence.
    pub frames: BattleFrameState,
    /// Labels captured from the same successful candidate as this world.
    pub metadata: CommittedBattleMetadata,
    /// Outcomes from actual ticks executed in the latest active frame.
    pub notices: Vec<CommandNotice>,
    /// Revalidation rejections from that frame, separate from authoritative outcomes.
    pub rejected: Vec<RejectedSubmission>,
    /// Fatal simulation diagnostics retained after draining the authoritative output.
    pub diagnostics: Vec<sim_core::SimulationDiagnostic>,
    /// Actual outcomes whose malformed correlation caused a fatal pipeline error.
    pub unmatched_outcomes: Vec<super::driver::AdmissionOutcome>,
    /// Total actual authoritative outcomes observed in this generation.
    pub outcome_count: u64,
    /// Cached last authoritative outcome summary, changed only by new evidence.
    pub outcome_status: String,
    pub(super) input: TickBundle,
    pub(super) sequences: Vec<SubmissionSequence>,
    pub(super) ready: bool,
    pub(super) presentation_blocked: bool,
}

impl Default for BattleSession {
    fn default() -> Self {
        Self {
            world: None,
            published: None,
            commands: BattleCommandQueue::default(),
            clock: BattleTickClock::default(),
            frames: BattleFrameState::default(),
            metadata: CommittedBattleMetadata::default(),
            notices: Vec::new(),
            rejected: Vec::new(),
            diagnostics: Vec::new(),
            unmatched_outcomes: Vec::new(),
            outcome_count: 0,
            outcome_status: String::new(),
            input: TickBundle {
                tick: Tick(0),
                commands: Vec::new(),
                state_checksum: None,
            },
            sequences: Vec::new(),
            ready: false,
            presentation_blocked: false,
        }
    }
}

impl BattleSession {
    /// Computes the next commit identity before any scene or world mutation.
    /// # Errors
    /// Rejects exhausted generations instead of reusing an old identity.
    pub fn next_generation(&self) -> Result<SessionGeneration, PipelineError> {
        self.commands
            .generation()
            .map_or(Ok(SessionGeneration::FIRST), |generation| {
                generation.next().map_err(Into::into)
            })
    }

    /// Builds a complete candidate pipeline using the real initial snapshot writer.
    /// Media readiness is validated by the ordinary tick driver before controls act.
    /// # Errors
    /// Propagates identity/publication errors before the caller replaces its scene.
    pub fn prepare(
        generation: SessionGeneration,
        world: &mut SimWorld,
        metadata: CommittedBattleMetadata,
    ) -> Result<Self, PipelineError> {
        let mut session = Self {
            metadata,
            ..Self::default()
        };
        session.commands.activate(generation)?;
        world.write_render_snapshot(session.frames.initialize(generation)?);
        session.frames.finish_initialization()?;
        Ok(session)
    }

    /// Hands the candidate world to this session and publishes its first presentation state.
    #[must_use]
    pub fn with_world(mut self, world: SimWorld) -> Self {
        let mut published = PublishedBattleState::default();
        published.refresh(&world);
        self.published = Some(published);
        self.world = Some(world);
        self
    }

    /// Drops the committed world, which leaves the session with nothing to present.
    pub fn release_world(&mut self) {
        self.world = None;
        self.published = None;
    }

    /// Authoritative world of this generation, if one is committed.
    #[must_use]
    pub fn world(&self) -> Option<&SimWorld> {
        self.world.as_ref()
    }

    /// Mutable authoritative world, for external control between ticks.
    ///
    /// Presentation keeps the previous values until the next tick or [`Self::publish`].
    pub fn world_mut(&mut self) -> Option<&mut SimWorld> {
        self.world.as_mut()
    }

    /// Splits the session into the world and the queue that admits commands against it.
    pub fn world_and_commands(&mut self) -> (Option<&SimWorld>, &mut BattleCommandQueue) {
        (self.world.as_ref(), &mut self.commands)
    }

    /// Copies the committed world for presentation now, without waiting for a tick.
    pub fn publish(&mut self) {
        if let (Some(world), Some(published)) = (&self.world, &mut self.published) {
            published.refresh(world);
        }
    }

    /// Latest presentation copy of the world, if one is committed.
    #[must_use]
    pub fn published(&self) -> Option<&PublishedBattleState> {
        self.published.as_ref()
    }

    /// Advances the committed world and republishes it when a tick executed.
    ///
    /// Does nothing without a committed world.
    ///
    /// # Errors
    /// Propagates [`Self::advance`] failures. The world stays in the session.
    pub fn advance_committed(
        &mut self,
        elapsed: std::time::Duration,
        active: bool,
        paused: bool,
        speed: u32,
        validate_media: impl FnMut(
            &sim_types::RenderSnapshot,
        ) -> Result<(), crate::media::ClientMediaError>,
    ) -> Result<(), PipelineError> {
        // The world leaves `self` only because `advance` borrows both, and it always returns.
        let Some(mut world) = self.world.take() else {
            return Ok(());
        };
        let result = self.advance(&mut world, elapsed, active, paused, speed, validate_media);
        if let Some(published) = &mut self.published {
            if published.tick != world.current_tick {
                published.refresh(&world);
            }
        }
        self.world = Some(world);
        result
    }

    /// Controls are actionable only after the committed world's media is validated.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready && !self.presentation_blocked
    }

    /// Suspends scheduling and holds the exact current pose without stepping.
    pub fn suspend(&mut self) {
        self.clock.reset_phase();
        self.frames.hold_exact();
    }
}
