//! The recorder: per-frame timing and unit evidence from inside the real client's schedules.
//!
//! # Responsibility
//! [`install`] adds the [`FrameRecorder`] resource and five systems to the production App:
//! - `begin_frame` in `First`: the frame interval (wall time since the previous `First`) and the
//!   start of the main-world update;
//! - `hold_camera` in `PreUpdate`: keeps the production [`RtsCamera`] on the spec's pose;
//! - `before_ticks` and `after_ticks` in `Update`, around `BattleSet::AdvanceTicks`: the
//!   simulation's wall time and the ticks it executed this frame;
//! - `end_frame` in `Last`: the main-world update time, the live units of the presented snapshot,
//!   how many of them project into the real camera's view, the frame's combat events, and the
//!   phase logic (wait for readiness, warm up with the battle paused, measure, finish).
//!
//! # How it fits
//! It changes the game only through the controls the client exposes to tools: the Lab's pause
//! flag and the camera component. [`super::run`] reads the result through the [`RecorderHandle`]
//! after the App exits.
//!
//! # Invariants
//! - Measurement begins only after the battle world exists, its map sprites are loaded, every
//!   GPU pipeline is compiled, and `warmup_frames` more frames have rendered.
//! - The measured rows go into a vector preallocated for the whole window, so measuring
//!   allocates nothing; a window longer than the preallocation fails instead of growing it.
//! - On every measured frame, at least `min_in_view_fraction` of the live units are in view, the
//!   match is still in progress, and the battle has not failed; otherwise the run fails.
//! - The first measured frame's tick is the scenario's tick 0 and its unit count is checked
//!   against the scenario's.
//! - Limit: the order of systems within `First` and `Last` is not pinned, so the update time
//!   misses the few systems of those schedules that run before `begin_frame` or after `end_frame`.
//! - Combat events are counted as the growth of the timeline's pending-event queue across the
//!   tick driver: the driver queues every event of every tick it runs, and presentation removes
//!   them only later in the frame (`BattleSet::Present`).

use super::report::FrameSample;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::window::PrimaryWindow;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::runtime_battle::{BattleRenderStatus, MapPresentation};
use client::{LabState, RtsCamera};
use sim_types::{AnimState, MatchResult, RenderSnapshot, ShapeKind};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long the measured window lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementWindow {
    /// Until this many simulation ticks have executed.
    Ticks(u32),
    /// For this many frames (smoke runs).
    Frames(u32),
}

impl std::fmt::Display for MeasurementWindow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ticks(ticks) => write!(formatter, "{ticks} ticks"),
            Self::Frames(frames) => write!(formatter, "{frames} frames"),
        }
    }
}

/// What the recorder needs from the spec and the scenario.
#[derive(Debug, Clone, Copy)]
pub struct RecorderConfig {
    /// Mobile units the scenario places.
    pub expected_units: u32,
    /// Frames rendered between readiness and measurement.
    pub warmup_frames: u32,
    /// Length of the measured window.
    pub window: MeasurementWindow,
    /// Camera ground target `(x, z)`, world units.
    pub camera_target: Vec2,
    /// Camera distance, world units.
    pub camera_distance: f32,
    /// Smallest share of live units in view on every measured frame.
    pub min_in_view_fraction: f64,
    /// Rows preallocated for the window.
    pub capacity: usize,
}

/// Why a measurement failed while the client ran.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RecordingError {
    /// The battle reported a failure.
    #[error("the battle failed: {0}")]
    BattleFailed(String),
    /// A GPU pipeline failed to compile.
    #[error("a GPU pipeline failed to compile")]
    PipelineFailed,
    /// The battle world vanished after it had been ready.
    #[error("the battle world is missing")]
    MissingWorld,
    /// The scenario placed another number of units than it declares.
    #[error("the scenario placed {found} mobile units, expected {expected}")]
    WrongUnitCount {
        /// Units the scenario declares.
        expected: u32,
        /// Units the world holds.
        found: u32,
    },
    /// Too few live units were in the camera's view.
    #[error("measured frame {frame}: only {in_view} of {live} live units are in view")]
    NotInView {
        /// Measured frame index.
        frame: u32,
        /// Units in view.
        in_view: u32,
        /// Live units.
        live: u32,
    },
    /// The match ended before the window did.
    #[error("the match ended at tick {tick}, {measured} ticks into the window")]
    MatchEnded {
        /// Simulation tick.
        tick: u32,
        /// Ticks measured so far.
        measured: u32,
    },
    /// The window needs more rows than were preallocated.
    #[error("the window needs more than the {0} preallocated frames")]
    CapacityExceeded(usize),
    /// The production camera is missing.
    #[error("the production camera is missing")]
    MissingCamera,
}

/// What a finished measurement hands back.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedRun {
    /// Measured rows.
    pub samples: Vec<FrameSample>,
    /// Adapter name.
    pub adapter: String,
    /// Driver and driver information.
    pub driver: String,
    /// Primary window size in physical pixels.
    pub resolution: (u32, u32),
    /// Present mode of the primary window.
    pub present_mode: String,
    /// `/proc/loadavg` when the window ended.
    pub load_after: String,
}

/// Shared slot through which the recorder's result leaves the App.
#[derive(Debug, Clone, Default)]
pub struct RecorderHandle(Arc<Mutex<Option<Result<RecordedRun, RecordingError>>>>);

impl RecorderHandle {
    /// Takes the result, if the recorder finished.
    #[must_use]
    pub fn take(&self) -> Option<Result<RecordedRun, RecordingError>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    /// Stores the result.
    fn put(&self, result: Result<RecordedRun, RecordingError>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
    }
}

/// Where the recorder is in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Waiting for the world, the map sprites and the GPU pipelines.
    WaitingForReady,
    /// Rendering warm-up frames with the battle paused; the count left.
    WarmingUp(u32),
    /// Measuring since the given tick.
    Measuring {
        /// Tick of the window's start.
        start_tick: u32,
    },
    /// The result has been handed back.
    Done,
}

/// The recorder's state, a resource of the main world.
#[derive(Resource)]
pub struct FrameRecorder {
    config: RecorderConfig,
    phase: Phase,
    samples: Vec<FrameSample>,
    previous_first: Option<Instant>,
    frame_started: Instant,
    interval: Duration,
    simulation_started: Instant,
    simulation: Duration,
    tick_before: u32,
    ticks_executed: u32,
    pending_events_before: usize,
    combat_events: u32,
    handle: RecorderHandle,
}

/// Adds the recorder to `app` and returns the handle that receives its result.
pub fn install(app: &mut App, config: RecorderConfig) -> RecorderHandle {
    let handle = RecorderHandle::default();
    let now = Instant::now();
    // PERFORMANCE RATIONALE:
    // Why: an allocation while measuring would add its own cost to the frames it measures.
    // What: every row of the window is reserved before the App runs.
    // Trade-off: the reservation is sized for the longest window the spec allows (about 64 bytes
    // per frame), and a longer window fails instead of growing the vector.
    let samples = Vec::with_capacity(config.capacity);
    app.insert_resource(FrameRecorder {
        config,
        phase: Phase::WaitingForReady,
        samples,
        previous_first: None,
        frame_started: now,
        interval: Duration::ZERO,
        simulation_started: now,
        simulation: Duration::ZERO,
        tick_before: 0,
        ticks_executed: 0,
        pending_events_before: 0,
        combat_events: 0,
        handle: handle.clone(),
    })
    .add_systems(First, begin_frame)
    .add_systems(PreUpdate, hold_camera)
    .add_systems(
        Update,
        before_ticks
            .after(BattleSet::CommitSession)
            .before(BattleSet::AdvanceTicks),
    )
    // Between the driver and presentation, which drains the pending combat events.
    .add_systems(
        Update,
        after_ticks
            .after(BattleSet::AdvanceTicks)
            .before(BattleSet::PreparePresentation),
    )
    .add_systems(Last, end_frame);
    handle
}

/// The simulation tick, if the battle world exists.
fn world_tick(session: &BattleSession) -> Option<u32> {
    session.world().map(|world| world.current_tick)
}

fn begin_frame(mut recorder: ResMut<'_, FrameRecorder>) {
    let now = Instant::now();
    recorder.interval = recorder
        .previous_first
        .map_or(Duration::ZERO, |previous| now - previous);
    recorder.previous_first = Some(now);
    recorder.frame_started = now;
    recorder.simulation = Duration::ZERO;
    recorder.ticks_executed = 0;
    recorder.combat_events = 0;
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn hold_camera(recorder: Res<'_, FrameRecorder>, mut cameras: Query<'_, '_, &mut RtsCamera>) {
    for mut camera in &mut cameras {
        if camera.target != recorder.config.camera_target
            || camera.distance.to_bits() != recorder.config.camera_distance.to_bits()
        {
            camera.target = recorder.config.camera_target;
            camera.distance = recorder.config.camera_distance;
        }
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn before_ticks(mut recorder: ResMut<'_, FrameRecorder>, session: Res<'_, BattleSession>) {
    recorder.tick_before = world_tick(&session).unwrap_or(0);
    recorder.pending_events_before = session.frames.pending_event_count();
    recorder.simulation_started = Instant::now();
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn after_ticks(mut recorder: ResMut<'_, FrameRecorder>, session: Res<'_, BattleSession>) {
    recorder.simulation = recorder.simulation_started.elapsed();
    recorder.ticks_executed = world_tick(&session)
        .unwrap_or(0)
        .saturating_sub(recorder.tick_before);
    let queued = session
        .frames
        .pending_event_count()
        .saturating_sub(recorder.pending_events_before);
    recorder.combat_events = u32::try_from(queued).unwrap_or(u32::MAX);
}

/// Everything `end_frame` reads or writes besides the recorder.
#[derive(SystemParam)]
struct FrameEvidence<'w, 's> {
    session: Res<'w, BattleSession>,
    lab: ResMut<'w, LabState>,
    render_status: Option<Res<'w, BattleRenderStatus>>,
    map_presentation: Option<Res<'w, MapPresentation>>,
    images: Res<'w, Assets<Image>>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<RtsCamera>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    adapter: Option<Res<'w, RenderAdapterInfo>>,
    exit: EventWriter<'w, AppExit>,
}

fn end_frame(mut recorder: ResMut<'_, FrameRecorder>, mut evidence: FrameEvidence<'_, '_>) {
    let update = recorder.frame_started.elapsed();
    if recorder.phase == Phase::Done {
        return;
    }
    let step = match recorder.phase {
        Phase::WaitingForReady => wait_for_ready(&evidence)
            .map(|ready| ready.then_some(Phase::WarmingUp(recorder.config.warmup_frames))),
        Phase::WarmingUp(left) => warm_up(&recorder, &mut evidence, left).map(Some),
        Phase::Measuring { start_tick } => measure(&mut recorder, &evidence, update, start_tick),
        Phase::Done => Ok(None),
    };
    match step {
        Ok(Some(Phase::Done)) => finish(&mut recorder, &mut evidence),
        Ok(Some(next)) => {
            if std::mem::discriminant(&next) != std::mem::discriminant(&recorder.phase) {
                info!("frame_budget: {:?} -> {next:?}", recorder.phase);
            }
            recorder.phase = next;
        }
        Ok(None) => {}
        Err(error) => {
            evidence.lab.battle_paused = true;
            recorder.handle.put(Err(error));
            recorder.phase = Phase::Done;
            evidence.exit.send(AppExit::error());
        }
    }
}

/// Fails on a reported battle or pipeline failure, then says whether the battle is ready.
fn wait_for_ready(evidence: &FrameEvidence<'_, '_>) -> Result<bool, RecordingError> {
    check_health(evidence)?;
    let sprites_loaded = evidence
        .map_presentation
        .as_ref()
        .is_some_and(|presentation| {
            presentation
                .sprite_images
                .iter()
                .all(|handle| evidence.images.contains(handle))
        });
    let pipelines_ready = evidence
        .render_status
        .as_ref()
        .is_some_and(|status| status.is_ready());
    Ok(evidence.session.world().is_some() && sprites_loaded && pipelines_ready)
}

/// The battle's and the GPU's reported failures.
fn check_health(evidence: &FrameEvidence<'_, '_>) -> Result<(), RecordingError> {
    if let Some(failure) = &evidence.lab.battle_failure {
        return Err(RecordingError::BattleFailed(failure.clone()));
    }
    if evidence
        .render_status
        .as_ref()
        .is_some_and(|status| status.failed())
    {
        return Err(RecordingError::PipelineFailed);
    }
    Ok(())
}

/// Counts down the warm-up; on its last frame checks the unit count and starts the battle.
fn warm_up(
    recorder: &FrameRecorder,
    evidence: &mut FrameEvidence<'_, '_>,
    left: u32,
) -> Result<Phase, RecordingError> {
    check_health(evidence)?;
    if left > 0 {
        return Ok(Phase::WarmingUp(left - 1));
    }
    let world = evidence
        .session
        .world()
        .ok_or(RecordingError::MissingWorld)?;
    let found = world
        .entities
        .values()
        .filter(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|archetype| matches!(archetype.shape, ShapeKind::UnitSphere))
                && entity.is_alive()
        })
        .count();
    let found = u32::try_from(found).unwrap_or(u32::MAX);
    if found != recorder.config.expected_units {
        return Err(RecordingError::WrongUnitCount {
            expected: recorder.config.expected_units,
            found,
        });
    }
    let start_tick = world.current_tick;
    evidence.lab.battle_paused = false;
    Ok(Phase::Measuring { start_tick })
}

/// Records one measured frame and says whether the window has ended.
fn measure(
    recorder: &mut FrameRecorder,
    evidence: &FrameEvidence<'_, '_>,
    update: Duration,
    start_tick: u32,
) -> Result<Option<Phase>, RecordingError> {
    check_health(evidence)?;
    let tick = world_tick(&evidence.session).ok_or(RecordingError::MissingWorld)?;
    let frame = u32::try_from(recorder.samples.len()).unwrap_or(u32::MAX);
    let (camera, transform) = evidence
        .cameras
        .get_single()
        .map_err(|_| RecordingError::MissingCamera)?;
    let presented = evidence.session.frames.presented();
    let (live, in_view) = count_units(presented, camera, transform);
    if f64::from(in_view) < recorder.config.min_in_view_fraction * f64::from(live) {
        return Err(RecordingError::NotInView {
            frame,
            in_view,
            live,
        });
    }
    let current = evidence.session.frames.current();
    if !matches!(current.match_result, MatchResult::InProgress) {
        return Err(RecordingError::MatchEnded {
            tick,
            measured: tick - start_tick,
        });
    }
    if recorder.samples.len() == recorder.samples.capacity() {
        return Err(RecordingError::CapacityExceeded(
            recorder.samples.capacity(),
        ));
    }
    let micros = |duration: Duration| u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
    let sample = FrameSample {
        frame,
        interval_us: micros(recorder.interval),
        update_us: micros(update),
        simulation_us: micros(recorder.simulation),
        ticks_executed: recorder.ticks_executed,
        tick,
        live_units: live,
        units_in_view: in_view,
        combat_events: recorder.combat_events,
    };
    recorder.samples.push(sample);
    let ended = match recorder.config.window {
        MeasurementWindow::Frames(frames) => recorder.samples.len() >= frames as usize,
        MeasurementWindow::Ticks(ticks) => tick - start_tick >= ticks,
    };
    Ok(ended.then_some(Phase::Done))
}

/// Live mobile units of `snapshot`, and how many of them project into the camera's view.
///
/// A unit is live unless it plays its death action. Its world position is the bubble centre:
/// simulation `(x, y)` on Bevy's `(x, z)`, raised by its altitude and radius.
fn count_units(
    snapshot: &RenderSnapshot,
    camera: &Camera,
    transform: &GlobalTransform,
) -> (u32, u32) {
    let mut live = 0_u32;
    let mut in_view = 0_u32;
    for bubble in &snapshot.bubbles {
        if !matches!(bubble.shape, ShapeKind::UnitSphere) || bubble.anim_state == AnimState::Die {
            continue;
        }
        live += 1;
        let centre = Vec3::new(
            bubble.world_pos[0],
            bubble.world_pos[2] + bubble.radius,
            bubble.world_pos[1],
        );
        let visible = camera
            .world_to_ndc(transform, centre)
            .is_some_and(|ndc| ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && ndc.z > 0.0);
        in_view += u32::from(visible);
    }
    (live, in_view)
}

/// Hands the measured rows and the run's environment back and ends the App.
fn finish(recorder: &mut FrameRecorder, evidence: &mut FrameEvidence<'_, '_>) {
    let (adapter, driver) = evidence.adapter.as_ref().map_or_else(
        || ("unknown".to_owned(), "unknown".to_owned()),
        |info| {
            (
                info.name.clone(),
                format!("{} {}", info.driver, info.driver_info),
            )
        },
    );
    let (resolution, present_mode) = evidence.windows.get_single().map_or_else(
        |_| ((0, 0), "no primary window".to_owned()),
        |window| {
            (
                (window.physical_width(), window.physical_height()),
                format!("{:?}", window.present_mode),
            )
        },
    );
    let run = RecordedRun {
        samples: std::mem::take(&mut recorder.samples),
        adapter,
        driver,
        resolution,
        present_mode,
        load_after: super::read_load_average(),
    };
    info!("frame_budget: measured {} frames", run.samples.len());
    recorder.handle.put(Ok(run));
    recorder.phase = Phase::Done;
    evidence.lab.battle_paused = true;
    evidence.exit.send(AppExit::Success);
}
