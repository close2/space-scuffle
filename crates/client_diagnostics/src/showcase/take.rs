//! One take: the production client, a fixed 40 ms step, the director, the overlay and the recorder.
//!
//! # Responsibility
//! [`run_take`] builds the shipped client in the take's mode on the take's scenario, waits with time
//! frozen until the renderer, the media and the first battle tick are ready, then renders the
//! storyboard's scenes frame by frame with `TimeUpdateStrategy::ManualDuration(40 ms)`, reads every
//! frame back and streams it into the lossless mezzanine (plan
//! `docs/plans/2026-11-05-ci-showcase-video.md`, decisions D-2, D-5 and D-8).
//!
//! # Schedule
//! - `PreUpdate`: egui input cleared and given a deterministic clock (shared with the capture tool).
//! - `Update`, `BattleSet::ExternalControl`: the director sets speed, input and camera.
//! - `Update`: the overlay paints the scene's text.
//! - `PostUpdate`: the camera transform takes the director's yaw.
//! - `Last`: readiness, then per frame the screenshot request, combat tracking and scene changes.
//! - Render world: pipeline readiness evidence, the UI attachment of screenshots, and
//!   [`complete_pipelines`], which finishes pipeline compilation before the frame is drawn.
//!
//! # Invariants
//! - A slow GPU only makes a take slower: every frame advances the same virtual 40 ms, and a frame
//!   is drawn only after every pipeline it queued has compiled.
//! - A recorded frame drawn while a pipeline still waited for its shader fails the take; nothing is
//!   recorded silently incomplete.
//! - The take never changes the simulation; it reads the session's world only.

use super::director::{self, CameraDirector, CameraYaw, CombatTracker, FrameOutcome, SceneClock};
use super::overlay::{self, CardTexts, OverlayState};
use super::recorder::{
    AdapterRecord, FrameOrderBuffer, FrameRecord, Mezzanine, SceneRecord, TakeManifest,
    MANIFEST_VERSION,
};
use super::storyboard::{Length, Take};
use super::{ShowcaseError, ValidatedStoryboard};
use crate::runtime_capture::pipelines::PipelineEvidence;
use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache};
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSet};
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, PrimaryWindow};
use client::battle_pipeline::session::BattleSession;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The fixed virtual step of every recorded frame.
pub const FRAME_STEP: Duration = Duration::from_millis(40);

/// Consecutive ready frames required before recording starts.
const SETTLE_FRAMES: u32 = 4;

/// Wall-clock seconds allowed for the last readbacks after the last frame.
const DRAIN_SECONDS: u64 = 120;

/// Wall-clock seconds one frame may wait for its pipelines to compile.
const PIPELINE_WAIT_SECONDS: u64 = 600;

/// The environment variable of the terrain anisotropy override (plan decision D-12, Q0009).
pub const ANISOTROPY_VARIABLE: &str = "SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY";

/// Shader bits of [`PipelineEvidence`] a take waits for: terrain and bubbles. Combat effects
/// compile on first use, which [`complete_pipelines`] makes synchronous.
const READY_SHADERS: u8 = 0b011;

/// What `showcase take` renders.
#[derive(Debug, Clone)]
pub struct TakeOptions {
    /// Storyboard path, relative to the assets root unless absolute.
    pub storyboard: PathBuf,
    /// Take name.
    pub take: String,
    /// Output directory; `<take>.mkv` and `<take>.manifest.ron` must not exist in it.
    pub out: PathBuf,
    /// Commit rendered, shown in the footer.
    pub revision: String,
    /// Commit date, `YYYY-MM-DD`, shown in the footer.
    pub revision_date: String,
    /// Repository root holding `DESIGN.md`; defaults to the parent of the assets root.
    pub repository: Option<PathBuf>,
}

/// Fails unless `tool` runs (`<tool> -version`).
///
/// # Errors
/// [`ShowcaseError::MissingTool`].
pub fn require_tool(tool: &'static str) -> Result<(), ShowcaseError> {
    let found = std::process::Command::new(tool)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if found {
        Ok(())
    } else {
        Err(ShowcaseError::MissingTool(tool))
    }
}

/// Renders one take. Returns the path of its manifest.
///
/// # Errors
/// Any start-up, storyboard or mezzanine error, or [`ShowcaseError::Take`] with the reason the take
/// failed; the manifest is then written with `complete: false`.
pub fn run_take(options: &TakeOptions) -> Result<PathBuf, ShowcaseError> {
    require_tool("ffmpeg")?;
    let root = client::ResolvedAssetsRoot::resolve()?;
    let assets = root.as_path().to_owned();
    let repository = match &options.repository {
        Some(path) => path.clone(),
        None => assets.parent().map(Path::to_owned).ok_or_else(|| {
            ShowcaseError::Take(format!("assets root {} has no parent", assets.display()))
        })?,
    };
    let validated = ValidatedStoryboard::load(&assets, &repository, &options.storyboard)?;
    let take = validated
        .storyboard
        .take(&options.take)
        .ok_or_else(|| ShowcaseError::UnknownTake(options.take.clone()))?
        .clone();
    let mode = take
        .mode
        .client_mode()
        .ok_or_else(|| ShowcaseError::UnknownTake(options.take.clone()))?;
    std::fs::create_dir_all(&options.out).map_err(|source| ShowcaseError::Io {
        path: options.out.clone(),
        source,
    })?;
    let mezzanine_path = options.out.join(format!("{}.mkv", take.name));
    let manifest_path = TakeManifest::path(&options.out, &take.name);
    if manifest_path.exists() {
        return Err(ShowcaseError::Take(format!(
            "refusing to overwrite {}",
            manifest_path.display()
        )));
    }
    let mut app = client::build_graphical_app(mode, None, &take.scenario)?;
    let (width, height) = validated.storyboard.resolution;
    configure_window(&mut app, width, height)?;
    let mezzanine = Mezzanine::start(
        &mezzanine_path,
        width,
        height,
        validated.storyboard.frame_rate,
    )?;
    let outcome = TakeOutcome::default();
    let state = TakeState::new(
        &validated,
        take,
        options,
        manifest_path.clone(),
        mezzanine,
        outcome.clone(),
    );
    install(&mut app, state, &validated, options);
    let exit = client::render_compatibility::run_render_app(&mut app);
    let result = outcome
        .0
        .lock()
        .map_err(|_| ShowcaseError::Take("the take's outcome lock was poisoned".into()))?
        .clone();
    match result {
        Some(Ok(())) if exit.is_success() => Ok(manifest_path),
        Some(Err(reason)) => Err(ShowcaseError::Take(reason)),
        _ => Err(ShowcaseError::Take(format!(
            "the client exited ({exit:?}) before the take completed"
        ))),
    }
}

/// Sets the window to `width`×`height` physical pixels at scale factor 1, without vsync.
fn configure_window(app: &mut App, width: u32, height: u32) -> Result<(), ShowcaseError> {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    let mut window = query
        .get_single_mut(app.world_mut())
        .map_err(|error| ShowcaseError::Take(format!("primary window: {error}")))?;
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set_physical_resolution(width, height);
    window.present_mode = PresentMode::AutoNoVsync;
    window.title = "Space Scuffle — showcase".into();
    Ok(())
}

/// Adds the take's resources and systems to the production app.
fn install(
    app: &mut App,
    state: TakeState,
    validated: &ValidatedStoryboard,
    options: &TakeOptions,
) {
    let cards = CardTexts {
        title: validated.cards.title.clone(),
        status: validated.cards.status.clone(),
        revision: options.revision.chars().take(7).collect(),
        revision_date: options.revision_date.clone(),
        adapter: String::new(),
    };
    let ledger = StallLedger::default();
    let evidence = PipelineEvidence::new(app.world().resource::<AssetServer>());
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = true;
    app.insert_resource(state)
        .insert_resource(cards)
        .insert_resource(OverlayState::default())
        .insert_resource(FrameStamp::default())
        .insert_resource(ledger.clone())
        .insert_resource(client::media::MediaFailurePolicy::new(true))
        .insert_resource(evidence.clone())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .add_systems(
            PreUpdate,
            (isolate_ui_input, scale_ui)
                .after(bevy_egui::EguiPreUpdateSet::ProcessInput)
                .before(bevy_egui::EguiPreUpdateSet::BeginPass),
        )
        .add_systems(
            Update,
            control.in_set(client::battle_pipeline::BattleSet::ExternalControl),
        )
        .add_systems(
            Update,
            overlay::draw_overlay.after(client::battle_pipeline::BattleSet::ExternalControl),
        )
        .add_systems(
            PostUpdate,
            director::apply_camera_yaw.before(TransformSystem::TransformPropagate),
        )
        .add_systems(Last, observe);
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(evidence)
            .insert_resource(ledger)
            .insert_resource(FrameStamp::default())
            .add_systems(ExtractSchedule, extract_stamp)
            .add_systems(Render, crate::runtime_capture::pipelines::inspect)
            .add_systems(
                Render,
                complete_pipelines
                    .in_set(RenderSet::Render)
                    .before(bevy::render::renderer::render_system),
            )
            .add_systems(
                Render,
                crate::runtime_capture::ui_attachment::align_ui_attachment
                    .after(bevy::render::view::prepare_view_targets)
                    .before(RenderSet::Render),
            );
    }
}

/// How the take ended, shared with [`run_take`] after the app is gone.
#[derive(Clone, Default)]
struct TakeOutcome(Arc<Mutex<Option<Result<(), String>>>>);

impl TakeOutcome {
    fn set(&self, result: Result<(), String>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(result);
        } else {
            error!("the take's outcome lock was poisoned");
        }
    }
}

/// The video frame whose screenshot this update requests, extracted into the render world.
#[derive(Resource, Clone, Copy, Default)]
struct FrameStamp(Option<u64>);

/// Recorded frames that were drawn while a pipeline still waited for its shader.
#[derive(Resource, Clone, Default)]
struct StallLedger(Arc<Mutex<BTreeSet<u64>>>);

impl StallLedger {
    fn insert(&self, frame: u64) {
        if let Ok(mut frames) = self.0.lock() {
            frames.insert(frame);
        } else {
            error!("the stall ledger lock was poisoned");
        }
    }

    fn contains(&self, frame: u64) -> bool {
        self.0.lock().map_or(true, |frames| frames.contains(&frame))
    }
}

/// Stage of a take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Loading,
    Recording,
    Draining,
    Complete,
    Failed,
}

/// A frame's record travelling with its screenshot request.
#[derive(Component, Clone)]
struct ShowcaseFrame(FrameRecord);

/// Everything the take's systems share.
#[derive(Resource)]
struct TakeState {
    take: Take,
    manifest: TakeManifest,
    manifest_path: PathBuf,
    stage: Stage,
    clock: SceneClock,
    camera: CameraDirector,
    combat: CombatTracker,
    loading_frames: u32,
    ready_streak: u32,
    ui_settled: bool,
    started: Instant,
    recording_started: Option<Instant>,
    last_frame_at: Option<Instant>,
    drain_deadline: Option<Instant>,
    total_frames: Option<u64>,
    buffer: FrameOrderBuffer<(FrameRecord, Vec<u8>)>,
    mezzanine: Option<Mezzanine>,
    outcome: TakeOutcome,
}

impl TakeState {
    fn new(
        validated: &ValidatedStoryboard,
        take: Take,
        options: &TakeOptions,
        manifest_path: PathBuf,
        mezzanine: Mezzanine,
        outcome: TakeOutcome,
    ) -> Self {
        let manifest = TakeManifest {
            format_version: MANIFEST_VERSION,
            complete: false,
            failure: None,
            take: take.name.clone(),
            storyboard: validated.path.clone(),
            scenario: take.scenario.clone(),
            mezzanine: format!("{}.mkv", take.name),
            resolution: validated.storyboard.resolution,
            frame_rate: validated.storyboard.frame_rate,
            adapter: AdapterRecord::default(),
            anisotropy_override: std::env::var(ANISOTROPY_VARIABLE).ok(),
            revision: options.revision.clone(),
            revision_date: options.revision_date.clone(),
            readiness_seconds: 0.0,
            render_seconds: 0.0,
            scenes: Vec::new(),
            frames: Vec::new(),
        };
        Self {
            take,
            manifest,
            manifest_path,
            stage: Stage::Loading,
            clock: SceneClock::default(),
            camera: CameraDirector::default(),
            combat: CombatTracker::default(),
            loading_frames: 0,
            ready_streak: 0,
            ui_settled: false,
            started: Instant::now(),
            recording_started: None,
            last_frame_at: None,
            drain_deadline: None,
            total_frames: None,
            buffer: FrameOrderBuffer::default(),
            mezzanine: Some(mezzanine),
            outcome,
        }
    }

    /// Ends the take as failed: stops `ffmpeg`, writes the incomplete manifest, reports the reason.
    fn fail(&mut self, reason: &str) -> AppExit {
        if self.stage == Stage::Failed {
            return AppExit::error();
        }
        error!("Showcase take failed: {reason}");
        self.stage = Stage::Failed;
        if let Some(mezzanine) = self.mezzanine.take() {
            mezzanine.abort();
        }
        self.manifest.failure = Some(reason.to_owned());
        self.manifest.complete = false;
        if let Err(error) = self.manifest.save(&self.manifest_path) {
            error!("Cannot write the failed take's manifest: {error}");
        }
        self.outcome.set(Err(reason.to_owned()));
        AppExit::error()
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn isolate_ui_input(
    mut inputs: Query<'_, '_, (&mut bevy_egui::EguiInput, &mut bevy_egui::EguiContext)>,
    mut state: ResMut<'_, TakeState>,
    time: Res<'_, Time>,
) {
    let startup = f64::from(state.loading_frames) * FRAME_STEP.as_secs_f64();
    if let Some(settled) =
        crate::runtime_capture::isolate_egui_inputs(&mut inputs, time.elapsed_secs_f64(), startup)
    {
        state.ui_settled = settled;
    }
}

/// Scales egui so its layout in points fits [`overlay::LAYOUT_LINES`] lines.
fn scale_ui(
    mut windows: Query<'_, '_, (&Window, &mut bevy_egui::EguiContextSettings), With<PrimaryWindow>>,
) {
    for (window, mut settings) in &mut windows {
        #[expect(
            clippy::cast_precision_loss,
            reason = "window heights are at most 4,096 pixels"
        )]
        let scale = window.resolution.physical_height() as f32 / overlay::LAYOUT_LINES;
        if (settings.scale_factor - scale).abs() > f32::EPSILON {
            settings.scale_factor = scale;
        }
    }
}

/// The director's exclusive control step, in `BattleSet::ExternalControl`.
fn control(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<'_, TakeState>| {
        let state = &mut *state;
        if state.stage != Stage::Recording {
            world.resource_mut::<client::LabState>().battle_paused = true;
            director::clear_input(world);
            *world.resource_mut::<OverlayState>() = OverlayState::default();
            return;
        }
        let scene = &state.take.scenes[state.clock.scene];
        let result = director::direct_frame(
            world,
            scene,
            state.clock.scene_frame,
            &mut state.camera,
            &state.combat,
        );
        *world.resource_mut::<OverlayState>() = OverlayState {
            overlay: Some(scene.overlay.clone()),
            badge: scene.badge.clone(),
            scene_frame: state.clock.scene_frame,
            length: match scene.length {
                Length::Frames(frames) => Some(frames),
                Length::UntilCombat { .. } => None,
            },
        };
        if let Err(error) = result {
            let exit = state.fail(&error.to_string());
            world.send_event(exit);
        }
    });
}

/// The take's `Last` step: health, readiness, recording and draining.
fn observe(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<'_, TakeState>| {
        let state = &mut *state;
        world.resource_mut::<FrameStamp>().0 = None;
        if matches!(state.stage, Stage::Complete | Stage::Failed) {
            return;
        }
        let result = check_health(world).and_then(|()| match state.stage {
            Stage::Loading => observe_loading(world, state),
            Stage::Recording => record_frame(world, state),
            Stage::Draining => observe_draining(world, state),
            Stage::Complete | Stage::Failed => Ok(()),
        });
        if let Err(reason) = result {
            let exit = state.fail(&reason);
            world.send_event(exit);
        }
    });
}

/// Every failure the production client reports.
fn check_health(world: &World) -> Result<(), String> {
    if world.resource::<PipelineEvidence>().failed() {
        return Err("a production GPU pipeline failed".into());
    }
    if let Some(error) = &world.resource::<client::LabState>().battle_failure {
        return Err(format!("battle failure: {error}"));
    }
    if let Some(error) = world
        .get_resource::<client::media::ClientMediaStatus>()
        .and_then(client::media::ClientMediaStatus::failure)
    {
        return Err(format!("media failure: {error}"));
    }
    if let Some(error) = world
        .get_resource::<client::player_presentation::PresentationStatus>()
        .and_then(client::player_presentation::PresentationStatus::failure_text)
    {
        return Err(format!("presentation failure: {error}"));
    }
    if world
        .get_resource::<client::runtime_battle::BattleRenderStatus>()
        .is_some_and(client::runtime_battle::BattleRenderStatus::failed)
    {
        return Err("a battle render pipeline failed".into());
    }
    if let Some(simulation) = world.resource::<BattleSession>().world() {
        if !simulation.diagnostics().is_empty() {
            return Err(format!(
                "simulation diagnostics: {:?}",
                simulation.diagnostics()
            ));
        }
    }
    Ok(())
}

/// Waits, with time frozen, until the scene can be drawn completely; then starts the 40 ms step.
fn observe_loading(world: &mut World, state: &mut TakeState) -> Result<(), String> {
    state.loading_frames += 1;
    if state.started.elapsed().as_secs() > state.take.readiness_timeout_seconds {
        return Err(format!(
            "take `{}` was not ready after {} s",
            state.take.name, state.take.readiness_timeout_seconds
        ));
    }
    let Some(simulation) = world.resource::<BattleSession>().world() else {
        state.ready_streak = 0;
        return Ok(());
    };
    if simulation.current_tick != 0 {
        return Err("the simulation advanced during start-up".into());
    }
    let sprites = world
        .get_resource::<client::runtime_battle::MapPresentation>()
        .is_some_and(|presentation| {
            let images = world.resource::<Assets<Image>>();
            presentation
                .sprite_images
                .iter()
                .all(|handle| images.contains(handle))
        });
    let pipelines = world
        .get_resource::<client::runtime_battle::BattleRenderStatus>()
        .is_some_and(client::runtime_battle::BattleRenderStatus::is_ready);
    let ready = sprites
        && pipelines
        && state.ui_settled
        && world.resource::<PipelineEvidence>().ready(READY_SHADERS);
    state.ready_streak = if ready { state.ready_streak + 1 } else { 0 };
    if state.ready_streak < SETTLE_FRAMES {
        return Ok(());
    }
    state.combat.observe(simulation);
    let adapter = world
        .get_resource::<RenderAdapterInfo>()
        .map(|info| AdapterRecord {
            name: info.name.clone(),
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
            backend: format!("{:?}", info.backend),
        })
        .ok_or("the renderer reports no adapter")?;
    world
        .resource_mut::<CardTexts>()
        .adapter
        .clone_from(&adapter.name);
    state.manifest.adapter = adapter;
    state.manifest.readiness_seconds = state.started.elapsed().as_secs_f32();
    *world.resource_mut::<TimeUpdateStrategy>() = TimeUpdateStrategy::ManualDuration(FRAME_STEP);
    state.stage = Stage::Recording;
    let now = Instant::now();
    state.recording_started = Some(now);
    state.last_frame_at = Some(now);
    info!(
        "Showcase take `{}` ready after {:.1} s",
        state.take.name, state.manifest.readiness_seconds
    );
    Ok(())
}

/// Requests the screenshot of the frame just simulated, then ends the frame for the scene clock.
fn record_frame(world: &mut World, state: &mut TakeState) -> Result<(), String> {
    let (tick, checksum, combat) = {
        let simulation = world
            .resource::<BattleSession>()
            .world()
            .ok_or("the battle disappeared")?;
        let combat = state.combat.observe(simulation);
        (
            simulation.current_tick,
            simulation.compute_checksum(),
            combat,
        )
    };
    let index = state.clock.frame;
    if state.clock.scene_frame == 0 {
        state.manifest.scenes.push(SceneRecord {
            name: state.take.scenes[state.clock.scene].name.clone(),
            first_frame: index,
            frames: 0,
            start_tick: tick,
        });
    }
    if let Some(scene) = state.manifest.scenes.last_mut() {
        scene.frames += 1;
    }
    let now = Instant::now();
    let wall_ms = state
        .last_frame_at
        .replace(now)
        .map_or(0.0, |previous| (now - previous).as_secs_f32() * 1_000.0);
    let record = FrameRecord {
        index,
        tick,
        checksum,
        blake3: String::new(),
        wall_ms,
    };
    world
        .spawn((Screenshot::primary_window(), ShowcaseFrame(record)))
        .observe(readback);
    world.resource_mut::<FrameStamp>().0 = Some(index);
    match state
        .clock
        .end_frame(&state.take.scenes, combat)
        .map_err(|error| error.to_string())?
    {
        FrameOutcome::Continue => {}
        FrameOutcome::NextScene => state.camera.start_scene(),
        FrameOutcome::Finished => {
            state.total_frames = Some(state.clock.frame);
            state.stage = Stage::Draining;
            state.drain_deadline = Some(Instant::now() + Duration::from_secs(DRAIN_SECONDS));
            world.resource_mut::<client::LabState>().battle_paused = true;
            *world.resource_mut::<TimeUpdateStrategy>() =
                TimeUpdateStrategy::ManualDuration(Duration::ZERO);
            world.remove_resource::<CameraYaw>();
        }
    }
    if index.is_multiple_of(100) {
        info!(
            "Showcase take `{}`: frame {index}, tick {tick}",
            state.take.name
        );
    }
    Ok(())
}

/// After the last frame: waits for every readback, closes the mezzanine and writes the manifest.
fn observe_draining(world: &mut World, state: &mut TakeState) -> Result<(), String> {
    let total = state.total_frames.ok_or("draining without a frame count")?;
    if state.buffer.released() < total {
        if state
            .drain_deadline
            .is_some_and(|deadline| Instant::now() > deadline)
        {
            return Err(format!(
                "only {} of {total} frames were read back within {DRAIN_SECONDS} s",
                state.buffer.released()
            ));
        }
        return Ok(());
    }
    state
        .buffer
        .finish(total)
        .map_err(|fault| fault.to_string())?;
    state
        .mezzanine
        .take()
        .ok_or("the mezzanine was already closed")?
        .finish()
        .map_err(|error| error.to_string())?;
    state.manifest.render_seconds = state
        .recording_started
        .map_or(0.0, |started| started.elapsed().as_secs_f32());
    state.manifest.complete = true;
    state.manifest.save(&state.manifest_path)?;
    state.stage = Stage::Complete;
    state.outcome.set(Ok(()));
    info!(
        "Showcase take `{}` complete: {total} frames in {:.1} s",
        state.take.name, state.manifest.render_seconds
    );
    world.send_event(AppExit::Success);
    Ok(())
}

/// Receives one frame's pixels, hashes them and hands them to the mezzanine in order.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn readback(
    trigger: Trigger<'_, ScreenshotCaptured>,
    frames: Query<'_, '_, &ShowcaseFrame>,
    mut state: ResMut<'_, TakeState>,
    ledger: Res<'_, StallLedger>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if matches!(state.stage, Stage::Failed | Stage::Complete) {
        return;
    }
    let result = (|| -> Result<(), String> {
        let ShowcaseFrame(mut record) = frames
            .get(trigger.entity())
            .map_err(|_| "a screenshot arrived without its frame record".to_owned())?
            .clone();
        if ledger.contains(record.index) {
            return Err(format!(
                "frame {} was drawn while a pipeline still waited for its shader",
                record.index
            ));
        }
        let image = trigger
            .event()
            .0
            .clone()
            .try_into_dynamic()
            .map_err(|error| format!("frame {}: {error}", record.index))?
            .to_rgb8();
        let (width, height) = state.manifest.resolution;
        if image.dimensions() != (width, height) {
            return Err(format!(
                "frame {} is {}x{}, the storyboard asks for {width}x{height}",
                record.index,
                image.width(),
                image.height()
            ));
        }
        let pixels = image.into_raw();
        record.blake3 = blake3::hash(&pixels).to_hex().to_string();
        let released = state
            .buffer
            .push(record.index, (record, pixels))
            .map_err(|fault| fault.to_string())?;
        for (index, (record, pixels)) in released {
            state
                .mezzanine
                .as_mut()
                .ok_or("the mezzanine is closed")?
                .write(index, &pixels)
                .map_err(|error| error.to_string())?;
            state.manifest.frames.push(record);
        }
        Ok(())
    })();
    if let Err(reason) = result {
        exit.send(state.fail(&reason));
    }
}

/// Copies the frame stamp of this update into the render world.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn extract_stamp(stamp: Extract<'_, '_, Res<'_, FrameStamp>>, mut render: ResMut<'_, FrameStamp>) {
    *render = **stamp;
}

// PERFORMANCE RATIONALE:
// Why: Bevy compiles pipelines on worker threads and draws a frame without the meshes whose
// pipeline is not ready, so a pipeline first needed mid-take (a combat effect) would leave holes in
// a few frames, and how many would depend on how fast the GPU driver compiles.
// What: before the frame is drawn, process the pipeline queue until no pipeline is still compiling;
// a recorded frame that still has a pipeline waiting for a shader is entered in the stall ledger.
// Trade-off: a frame that introduces a pipeline takes as long as its compilation (seconds on
// lavapipe), in exchange for frames that depend only on the frame index and the simulation.
/// Finishes every pipeline compilation before the frame is drawn.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn complete_pipelines(
    mut cache: ResMut<'_, PipelineCache>,
    stamp: Res<'_, FrameStamp>,
    ledger: Res<'_, StallLedger>,
    evidence: Res<'_, PipelineEvidence>,
) {
    let deadline = Instant::now() + Duration::from_secs(PIPELINE_WAIT_SECONDS);
    cache.process_queue();
    while cache
        .pipelines()
        .any(|pipeline| matches!(pipeline.state, CachedPipelineState::Creating(_)))
    {
        if Instant::now() > deadline {
            evidence.report_failure("a pipeline did not compile within the showcase's wait");
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
        cache.process_queue();
    }
    if let Some(frame) = stamp.0 {
        if cache.waiting_pipelines().next().is_some() {
            ledger.insert(frame);
        }
    }
}
