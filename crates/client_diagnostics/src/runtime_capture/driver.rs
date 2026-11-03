//! Capture-only control and evidence state machine over production client systems.
//!
//! Simulation advances exclusively in the production queued tick driver. Rendering waits
//! freeze virtual time and gameplay, while independent wall deadlines remain active.
//! Evidence clones/maps deliberately allocate in this tool; timings are not game profiling.

use super::{
    artifacts::{self, Manifest, Record},
    config::{CaptureError, CaptureScript, Goal},
    pipelines::{PipelineEvidence, SHADERS},
};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::TimeUpdateStrategy;
use client::{BubbleInstance, LabState, LabTab, LiveBattleBubble, RtsCamera};
use sim_types::{EntityId, SimScalar, SimVec2};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

mod verify;

use verify::verify_frame;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Loading,
    Apply,
    Advancing,
    Settle,
    Readback,
    AuditDrain,
    Complete,
    Failed,
}

#[derive(Clone, Copy)]
struct Prior {
    position: SimVec2,
    hp: SimScalar,
    shields: SimScalar,
    level: u8,
    ramp: bool,
    cell: (u32, u32),
}

#[derive(Clone, Copy)]
struct CombatProof {
    generation: u64,
    tick: u32,
    victim: EntityId,
    source: EntityId,
    victim_position: [f32; 3],
    source_position: [f32; 3],
    victim_owner: sim_types::PlayerId,
    source_owner: sim_types::PlayerId,
}

#[derive(Resource)]
pub(super) struct Session {
    audio_policy: client::presentation::AudioPolicy,
    pub(super) ui_settled: bool,
    pub(super) completed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) script: CaptureScript,
    pub(super) scenario: PathBuf,
    pub(super) script_path: PathBuf,
    pub(super) root: PathBuf,
    pub(super) out: PathBuf,
    media: super::media::ValidatedMedia,
    transport: Option<super::transport::TransportState>,
    ownership: Option<super::ownership::OwnershipState>,
    stage: Stage,
    index: usize,
    frames: u64,
    deadline: Instant,
    run_started: Instant,
    start_tick: u32,
    last_tick: u32,
    settled: u32,
    focus: Option<Vec2>,
    subject: Option<EntityId>,
    ramp_actor: Option<EntityId>,
    entry_level: Option<u8>,
    exit_level: Option<u8>,
    combat_source: Option<EntityId>,
    combat_tick: Option<u32>,
    combat_generation: Option<u64>,
    ramp_generation: Option<u64>,
    combat_proofs: Vec<CombatProof>,
    ramp_trace: Vec<artifacts::RampSample>,
    prior: BTreeMap<EntityId, Prior>,
    initial_entities: BTreeMap<EntityId, sim_core::Entity>,
    initial_checksum: Option<u64>,
    saved_camera: Option<RtsCamera>,
    saved_checksum: Option<u64>,
    pending: Option<(Entity, Record)>,
    audit_pending: Option<Record>,
    records: Vec<Record>,
    failure: Option<String>,
}

impl Session {
    pub(super) fn new(
        script: CaptureScript,
        scenario: PathBuf,
        script_path: PathBuf,
        root: PathBuf,
        out: PathBuf,
        audio_policy: client::presentation::AudioPolicy,
    ) -> Self {
        Self {
            audio_policy,
            ui_settled: false,
            completed: std::sync::Arc::default(),
            ownership: script
                .ownership
                .then(super::ownership::OwnershipState::default),
            script,
            scenario,
            script_path,
            root,
            out,
            media: super::media::ValidatedMedia::default(),
            transport: None,
            stage: Stage::Loading,
            index: 0,
            frames: 0,
            deadline: Instant::now(),
            run_started: Instant::now(),
            start_tick: 0,
            last_tick: 0,
            settled: 0,
            focus: None,
            subject: None,
            ramp_actor: None,
            entry_level: None,
            exit_level: None,
            combat_source: None,
            combat_tick: None,
            combat_generation: None,
            ramp_generation: None,
            combat_proofs: Vec::new(),
            ramp_trace: vec![],
            prior: BTreeMap::new(),
            initial_entities: BTreeMap::new(),
            initial_checksum: None,
            saved_camera: None,
            saved_checksum: None,
            pending: None,
            audit_pending: None,
            records: vec![],
            failure: None,
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "capture coordinates are bounded by the window size and frame budget"
    )]
    pub(super) fn ui_startup_seconds(&self) -> f64 {
        self.frames as f64 * 0.04
    }
    pub(super) fn manifest(&self) -> Result<(), CaptureError> {
        artifacts::write_manifest(&self.out, &Manifest { format_version:2, complete:self.stage==Stage::Complete,
            failure:self.failure.as_deref(), scenario:&self.scenario,script:&self.script_path,assets_root:&self.root,
            resolution:self.script.resolution,audio_policy:self.audio_policy,shader_paths:SHADERS,checkpoints:&self.records,
            control_contract:"Direct production state/control actions; OS key/mouse adapters are not verified" })
    }
    fn goal(&self) -> Goal {
        self.script.checkpoints[self.index].goal
    }
    fn settled_for_verification(&mut self, sample_ready: bool) -> bool {
        if !sample_ready || (self.goal().is_ownership() && self.focus.is_none()) {
            self.settled = 0;
            return false;
        }
        if !self.ui_settled {
            return false;
        }
        self.settled += 1;
        self.settled >= 4
    }
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Bevy passes system parameters by value"
    )]
    fn fail(&mut self, error: CaptureError, world: &mut World) {
        error!("{error}");
        self.failure = Some(error.to_string());
        self.stage = Stage::Failed;
        if let Some(audit) = world.get_resource::<crate::shell_order_audit::AuditBridge>() {
            if let Err(write_error) = save_audit_incomplete(audit, &self.out) {
                error!("Cannot preserve incomplete shell audit: {write_error}");
            }
        }
        if let Err(error) = self.manifest() {
            error!("Capture failure manifest: {error}");
        }
        world.send_event(AppExit::error());
    }
}

fn failure(message: impl Into<String>) -> CaptureError {
    CaptureError::Verification(message.into())
}

fn save_audit_incomplete(
    audit: &crate::shell_order_audit::AuditBridge,
    out: &std::path::Path,
) -> Result<(), CaptureError> {
    if let Some(report) = audit.incomplete()? {
        audit.persist_sources(out)?;
        write_audit(
            &out.join("shell-audit-incomplete.ron"),
            &report,
            audit.publication_limit()?,
        )?;
    }
    Ok(())
}

fn write_audit(
    path: &std::path::Path,
    report: &crate::shell_order_audit::interval::IntervalReport,
    byte_limit: usize,
) -> Result<(), CaptureError> {
    let text = report.encode_bounded(byte_limit)?;
    std::fs::write(path, text).map_err(|source| CaptureError::Io {
        path: path.into(),
        source,
    })
}

fn finish_audit(world: &mut World, session: &mut Session) -> Result<(), CaptureError> {
    let audit = world.resource::<crate::shell_order_audit::AuditBridge>();
    let Some(report) = audit.finish()? else {
        return Ok(());
    };
    let mut record = session
        .audit_pending
        .clone()
        .ok_or_else(|| failure("closed audit has no preserved screenshot"))?;
    let path = session.out.join(format!("{}-shell-order.ron", record.name));
    audit.persist_sources(&session.out)?;
    write_audit(&path, &report, audit.publication_limit()?)?;
    if !report.classification.starts_with("stable") {
        return Err(failure(report.classification));
    }
    record.shell_audit = Some(path);
    info!("Verified complete shell-order interval for {}", record.name);
    session.records.push(record);
    session.index += 1;
    session.stage = if session.index == session.script.checkpoints.len() {
        Stage::Complete
    } else {
        Stage::Apply
    };
    session.manifest()?;
    audit.acknowledge_publication()?;
    session.audit_pending = None;
    if session.stage == Stage::Complete {
        session
            .completed
            .store(true, std::sync::atomic::Ordering::Release);
        world.send_event(AppExit::Success);
    }
    Ok(())
}

fn camera(world: &mut World) -> Result<(RtsCamera, Transform), CaptureError> {
    world
        .query_filtered::<(&RtsCamera, &Transform), With<Camera3d>>()
        .get_single(world)
        .map(|(camera, transform)| (camera.clone(), *transform))
        .map_err(|error| failure(format!("production camera: {error}")))
}

fn render_position(world: &mut World, id: EntityId) -> Result<Vec2, CaptureError> {
    world
        .query::<(&LiveBattleBubble, &GlobalTransform)>()
        .iter(world)
        .find(|(bubble, _)| bubble.entity_id == id)
        .map(|(_, transform)| transform.translation().xz())
        .ok_or_else(|| {
            failure(format!(
                "tracked actor {id:?} has no production render entity"
            ))
        })
}

fn freeze(world: &mut World) {
    world.resource_mut::<LabState>().battle_paused = true;
    *world.resource_mut::<TimeUpdateStrategy>() =
        TimeUpdateStrategy::ManualDuration(Duration::ZERO);
}

/// Runs before normal lifecycle; transport goals invoke only real typed transactions.
pub(super) fn control(world: &mut World) {
    world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
    world.resource_mut::<ButtonInput<MouseButton>>().reset_all();
    world
        .resource_mut::<Events<bevy::input::mouse::MouseMotion>>()
        .clear();
    world
        .resource_mut::<Events<bevy::input::mouse::MouseWheel>>()
        .clear();
    world.resource_scope(|world, mut session: Mut<'_, Session>| {
        let result = control_inner(world, &mut session);
        if let Err(error) = result {
            session.fail(error, world);
        }
    });
}

/// One control step: applies a newly entered checkpoint, keeps gameplay frozen while
/// waiting, and holds the camera on the settling focus.
fn control_inner(world: &mut World, session: &mut Session) -> Result<(), CaptureError> {
    if matches!(session.stage, Stage::Complete | Stage::Failed) {
        return Ok(());
    }
    if session.stage == Stage::Apply && apply_checkpoint(world, session)? == Applied::Advancing {
        return Ok(());
    }
    if matches!(
        session.stage,
        Stage::Loading | Stage::Settle | Stage::Readback | Stage::AuditDrain
    ) {
        freeze(world);
    }
    settle_camera(world, session)
}

/// What applying a checkpoint left the driver doing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Applied {
    /// The checkpoint waits for its frame to settle under frozen gameplay.
    Settling,
    /// Gameplay was resumed so the simulation can advance to the checkpoint's condition.
    Advancing,
}

/// Freezes gameplay, clears the previous checkpoint's per-goal state, and performs the
/// production action of the current goal.
fn apply_checkpoint(world: &mut World, session: &mut Session) -> Result<Applied, CaptureError> {
    freeze(world);
    session.settled = 0;
    session.focus = None;
    session.subject = None;
    session.combat_proofs.clear();
    session.deadline = Instant::now();
    let goal = session.goal();
    if goal == Goal::RampEntry {
        session.ramp_generation = world
            .resource::<client::battle_pipeline::session::BattleSession>()
            .commands
            .generation()
            .map(client::battle_pipeline::commands::SessionGeneration::value);
    }
    match goal {
        Goal::NeutralWatchtower
        | Goal::Swarm
        | Goal::EffectHitscan
        | Goal::EffectProjectile
        | Goal::EffectLaser
        | Goal::EffectLightning
        | Goal::EffectLightningMiddle
        | Goal::EffectLightningLate
        | Goal::EffectMelee
        | Goal::EffectShatter => {
            session.focus = session
                .ownership
                .as_mut()
                .ok_or_else(|| failure("ownership profile state missing"))?
                .apply(world, goal.ownership_view(), goal.effect_elapsed())?;
            session.stage = Stage::Settle;
        }
        Goal::CargoBefore | Goal::CargoLoaded | Goal::CargoUnloaded => {
            return apply_cargo(world, session, goal);
        }
        Goal::Overview | Goal::Reset => {
            session.focus = Some(Vec2::from_array(session.script.overview_focus));
            if goal == Goal::Reset {
                world.resource_mut::<LabState>().battle_reset_requested = true;
            }
            session.stage = Stage::Settle;
        }
        Goal::Lab => {
            session.saved_camera = Some(camera(world)?.0);
            session.saved_checksum = world
                .resource::<client::battle_pipeline::session::BattleSession>()
                .world()
                .map(sim_core::SimWorld::compute_checksum);
            world.resource_mut::<LabState>().current_tab = LabTab::VisualLab;
            session.stage = Stage::Settle;
        }
        Goal::Return => {
            world.resource_mut::<LabState>().current_tab = LabTab::LiveBattle;
            session.stage = Stage::Settle;
        }
        Goal::RampEntry | Goal::RampExit | Goal::Combat => {
            let pipeline = world.resource::<client::battle_pipeline::session::BattleSession>();
            let simulation = pipeline
                .world()
                .ok_or_else(|| failure("battle disappeared"))?;
            session.start_tick = simulation.current_tick;
            session.last_tick = simulation.current_tick;
            update_prior(simulation, &mut session.prior)?;
            resume(world, session);
            // TimeUpdateStrategy applies next frame; no step is expected yet.
            session.start_tick = simulation_tick(world)?;
            return Ok(Applied::Advancing);
        }
    }
    Ok(Applied::Settling)
}

/// Selects the transport participants (before) or performs the real typed load or unload
/// transaction, then frames the fixed overview. The before checkpoint settles; a
/// transaction resumes gameplay so its tick is published.
fn apply_cargo(
    world: &mut World,
    session: &mut Session,
    goal: Goal,
) -> Result<Applied, CaptureError> {
    let mut pipeline = world.resource_mut::<client::battle_pipeline::session::BattleSession>();
    let simulation = pipeline
        .world_mut()
        .ok_or_else(|| failure("transport simulation missing"))?;
    if goal == Goal::CargoBefore {
        let spec = session
            .script
            .transport
            .as_ref()
            .ok_or_else(|| failure("transport script specification missing"))?;
        session.transport = Some(super::transport::TransportState::select(simulation, spec)?);
    } else {
        session
            .transport
            .as_mut()
            .ok_or_else(|| failure("transport selection missing"))?
            .apply(simulation, goal)?;
    }
    session.subject = Some(
        session
            .transport
            .as_ref()
            .ok_or_else(|| failure("transport selection missing"))?
            .carrier(),
    );
    session.focus = Some(Vec2::from_array(session.script.overview_focus));
    session.start_tick = simulation.current_tick;
    session.last_tick = simulation.current_tick;
    if goal == Goal::CargoBefore {
        session.stage = Stage::Settle;
        Ok(Applied::Settling)
    } else {
        resume(world, session);
        Ok(Applied::Advancing)
    }
}

/// Unpauses the battle with a fixed 40 ms virtual frame step and enters `Advancing`.
fn resume(world: &mut World, session: &mut Session) {
    world.resource_mut::<LabState>().battle_paused = false;
    *world.resource_mut::<TimeUpdateStrategy>() =
        TimeUpdateStrategy::ManualDuration(Duration::from_millis(40));
    session.stage = Stage::Advancing;
}

/// While settling, lets an ownership goal refine its focus, then points the production
/// camera at the focus from the checkpoint's distance in world units.
fn settle_camera(world: &mut World, session: &mut Session) -> Result<(), CaptureError> {
    if session.stage == Stage::Settle && session.goal().is_ownership() {
        let view = session.goal().ownership_view();
        if let Some(focus) = session
            .ownership
            .as_mut()
            .ok_or_else(|| failure("ownership profile state missing"))?
            .settle(world, view)?
        {
            session.focus = Some(focus);
        }
    }
    if session.stage == Stage::Settle {
        if let Some(focus) = session.focus {
            let mut query = world.query::<&mut RtsCamera>();
            let mut camera = query
                .get_single_mut(world)
                .map_err(|e| failure(e.to_string()))?;
            camera.target = focus;
            camera.distance = session.script.checkpoints[session.index].distance;
        }
    }
    Ok(())
}

fn simulation_tick(world: &World) -> Result<u32, CaptureError> {
    world
        .get_resource::<client::battle_pipeline::session::BattleSession>()
        .ok_or_else(|| failure("battle controls missing"))?
        .world()
        .map(|w| w.current_tick)
        .ok_or_else(|| failure("battle simulation missing"))
}

fn update_prior(
    world: &sim_core::SimWorld,
    prior: &mut BTreeMap<EntityId, Prior>,
) -> Result<(), CaptureError> {
    for (&id, entity) in &world.entities {
        let (x, y) = world
            .map
            .world_to_cell(entity.position)
            .map_err(|error| failure(error.to_string()))?;
        let cell = world
            .map
            .cell(x, y)
            .map_err(|error| failure(error.to_string()))?;
        prior.insert(
            id,
            Prior {
                position: entity.position,
                hp: entity.hp,
                shields: entity.shields,
                level: cell.cliff_level.0,
                ramp: cell.ramp.is_some(),
                cell: (x, y),
            },
        );
    }
    Ok(())
}

/// Observes actual propagated transforms and completed simulation updates.
pub(super) fn observe(world: &mut World) {
    world.resource_scope(|world, mut session: Mut<'_, Session>| {
        let result = observe_inner(world, &mut session);
        if let Err(error) = result {
            session.fail(error, world);
        }
    });
}

/// One observation step: checks every failure source and the wait budgets, then advances
/// the stage that is waiting on this frame.
fn observe_inner(world: &mut World, session: &mut Session) -> Result<(), CaptureError> {
    if matches!(session.stage, Stage::Complete | Stage::Failed) {
        return Ok(());
    }
    observe_failures(world, session)?;
    session.frames += 1;
    check_wait_budgets(world, session)?;
    if world.resource::<PipelineEvidence>().failed() {
        return Err(failure("production GPU pipeline failed"));
    }
    let lab = world.resource::<LabState>();
    if let Some(error) = &lab.battle_failure {
        return Err(failure(error.clone()));
    }
    let Some(simulation) = world
        .resource::<client::battle_pipeline::session::BattleSession>()
        .world()
    else {
        return Ok(());
    };
    if !simulation.diagnostics().is_empty() {
        return Err(failure(format!(
            "simulation diagnostics: {:?}",
            simulation.diagnostics()
        )));
    }
    match session.stage {
        Stage::AuditDrain => finish_audit(world, session),
        Stage::Loading => observe_loading(simulation, session),
        Stage::Advancing => observe_advancing(world, session),
        Stage::Settle => observe_settle(world, session),
        Stage::Apply | Stage::Readback | Stage::Complete | Stage::Failed => Ok(()),
    }
}

/// Surfaces failures reported by the shell audit, media loading, the transport and player
/// presentation, records ownership effects, and requires a frozen frame to stay frozen while
/// its screenshot or audit is pending.
fn observe_failures(world: &mut World, session: &mut Session) -> Result<(), CaptureError> {
    if let Some(audit) = world.get_resource::<crate::shell_order_audit::AuditBridge>() {
        audit.failure()?;
    }
    if let Some(error) = world
        .get_resource::<client::media::ClientMediaStatus>()
        .and_then(client::media::ClientMediaStatus::failure)
    {
        return Err(failure(error.to_string()));
    }
    if let Some(transport) = &mut session.transport {
        transport.observe_effects(world)?;
    }
    if let Some(error) = world
        .get_resource::<client::player_presentation::PresentationStatus>()
        .and_then(client::player_presentation::PresentationStatus::failure_text)
    {
        return Err(failure(error));
    }
    let view = session.goal().ownership_view();
    if let Some(ownership) = &mut session.ownership {
        ownership.observe(world, view)?;
    }
    if session.stage == Stage::Readback {
        if let Some((_, record)) = &session.pending {
            if let Some(proof) = &record.ownership {
                proof
                    .frozen
                    .verify(&super::ownership::Fingerprint::capture(world)?)?;
            }
        }
    }
    if let Some(record) = &session.audit_pending {
        if let Some(proof) = &record.ownership {
            proof
                .frozen
                .verify(&super::ownership::Fingerprint::capture(world)?)?;
        }
    }
    Ok(())
}

/// Fails once the rendered-frame budget, the per-checkpoint wall deadline or the whole-run
/// wall deadline (seconds per checkpoint times the checkpoint count) is exceeded.
fn check_wait_budgets(world: &World, session: &Session) -> Result<(), CaptureError> {
    if session.frames > session.script.max_frames
        || session.deadline.elapsed().as_secs() > session.script.timeout_seconds
        || session.run_started.elapsed().as_secs()
            > session.script.timeout_seconds * session.script.checkpoints.len() as u64
    {
        return Err(failure(format!(
            "checkpoint {} timed out after {} frames at tick {:?}",
            session.script.checkpoints[session.index].name,
            session.frames,
            simulation_tick(world)
        )));
    }
    Ok(())
}

/// During rendering warmup the simulation must still be at tick 0 and the overview focus
/// inside the map; the loaded state is then recorded as the scenario baseline.
#[expect(
    clippy::cast_precision_loss,
    reason = "capture coordinates are bounded by the window size and frame budget"
)]
fn observe_loading(
    simulation: &sim_core::SimWorld,
    session: &mut Session,
) -> Result<(), CaptureError> {
    if simulation.current_tick != 0 {
        return Err(failure(
            "startup advanced simulation during rendering warmup",
        ));
    }
    if session.script.overview_focus[0] >= simulation.map.width() as f32 - 0.5
        || session.script.overview_focus[1] >= simulation.map.height() as f32 - 0.5
    {
        return Err(failure("overview focus is outside the selected map"));
    }
    session.initial_entities = simulation.entities.clone();
    session.initial_checksum = Some(simulation.compute_checksum());
    session.stage = Stage::Apply;
    Ok(())
}

/// Follows the simulation one tick at a time until the goal's condition selects a subject,
/// then freezes and settles on it; fails if a tick is skipped or the tick budget runs out.
fn observe_advancing(world: &mut World, session: &mut Session) -> Result<(), CaptureError> {
    let tick = simulation_tick(world)?;
    if tick == session.last_tick {
        return Ok(());
    } // first frame after enabling manual time
    if tick != session.last_tick + 1 {
        return Err(failure(format!(
            "capture tick skipped: {} -> {tick}",
            session.last_tick
        )));
    }
    session.last_tick = tick;
    let selected = if session.goal().is_transport() {
        Some(
            session
                .transport
                .as_ref()
                .ok_or_else(|| failure("transport selection missing"))?
                .carrier(),
        )
    } else if session.goal() == Goal::Combat {
        correlated_combat(world, session)?
    } else {
        let simulation = world
            .resource::<client::battle_pipeline::session::BattleSession>()
            .world()
            .ok_or_else(|| failure("battle disappeared"))?;
        condition(simulation, session)?
    };
    update_prior(
        world
            .resource::<client::battle_pipeline::session::BattleSession>()
            .world()
            .ok_or_else(|| failure("battle disappeared"))?,
        &mut session.prior,
    )?;
    if let Some(id) = selected {
        session.subject = Some(id);
        if !session.goal().is_transport() {
            session.focus = Some(render_position(world, id)?);
        }
        session.settled = 0;
        session.stage = Stage::Settle;
        freeze(world);
    } else if tick - session.start_tick >= session.script.checkpoints[session.index].max_ticks {
        return Err(failure(format!(
            "{:?} condition did not occur within tick budget",
            session.goal()
        )));
    }
    Ok(())
}

/// Once the frame has settled, verifies it and requests the screenshot readback, arming the
/// shell audit first at the Swarm checkpoint.
fn observe_settle(world: &mut World, session: &mut Session) -> Result<(), CaptureError> {
    let sample_ready = if let super::ownership::View::Effect(kind) = session.goal().ownership_view()
    {
        session
            .ownership
            .as_ref()
            .ok_or_else(|| failure("effect state missing"))?
            .sample_ready(world, kind)?
    } else {
        true
    };
    if !session.settled_for_verification(sample_ready) {
        return Ok(());
    }
    if let Some(record) = verify_frame(world, session)? {
        let audit = if session.goal() == Goal::Swarm {
            world
                .get_resource::<crate::shell_order_audit::AuditBridge>()
                .cloned()
        } else {
            None
        };
        let nonce = if let Some(audit) = &audit {
            if !audit.ready()? {
                return Ok(());
            }
            Some(audit.arm()?)
        } else {
            None
        };
        let entity = world
            .spawn(Screenshot::primary_window())
            .observe(readback)
            .id();
        if let (Some(audit), Some(nonce)) = (audit, nonce) {
            audit.bind(nonce, entity)?;
        }
        session.pending = Some((entity, record));
        session.stage = Stage::Readback;
    }
    Ok(())
}

fn condition(
    world: &sim_core::SimWorld,
    session: &mut Session,
) -> Result<Option<EntityId>, CaptureError> {
    match session.goal() {
        Goal::RampEntry => {
            for (&id, entity) in &world.entities {
                let Some(prior) = session.prior.get(&id) else {
                    continue;
                };
                if entity.owner.0 != session.script.ramp_player
                    || entity.position == prior.position
                    || prior.ramp
                {
                    continue;
                }
                let archetype = world
                    .catalog
                    .get(entity.unit_type_id)
                    .map_err(|error| failure(error.to_string()))?;
                if archetype.mover_type != sim_types::MoverType::Ground
                    || archetype.max_speed <= SimScalar::ZERO
                    || archetype.weapon.is_none() && archetype.ground_weapon.is_none()
                {
                    continue;
                }
                let (x, y) = world
                    .map
                    .world_to_cell(entity.position)
                    .map_err(|error| failure(error.to_string()))?;
                if world
                    .map
                    .cell(x, y)
                    .map_err(|error| failure(error.to_string()))?
                    .ramp
                    .is_some()
                {
                    session.ramp_actor = Some(id);
                    session.entry_level = Some(prior.level);
                    session.ramp_trace.push(artifacts::RampSample {
                        tick: world.current_tick - 1,
                        cell: [prior.cell.0, prior.cell.1],
                        level: prior.level,
                        ramp: false,
                        position_raw: [prior.position.x.raw(), prior.position.y.raw()],
                    });
                    session.ramp_trace.push(ramp_sample(
                        world.current_tick,
                        entity.position,
                        (x, y),
                        world.map.cell(x, y).map_err(|e| failure(e.to_string()))?,
                    ));
                    return Ok(Some(id));
                }
            }
        }
        Goal::RampExit => {
            let id = session
                .ramp_actor
                .ok_or_else(|| failure("ramp exit has no tracked entry actor"))?;
            let entity = world
                .entities
                .get(&id)
                .ok_or_else(|| failure("ramp actor died before verified traversal"))?;
            let (x, y) = world
                .map
                .world_to_cell(entity.position)
                .map_err(|error| failure(error.to_string()))?;
            let cell = world
                .map
                .cell(x, y)
                .map_err(|error| failure(error.to_string()))?;
            let prior = session
                .prior
                .get(&id)
                .ok_or_else(|| failure("tracked ramp actor has no preceding sample"))?;
            let exit = verify_ramp_step(
                prior,
                (x, y),
                cell,
                session
                    .entry_level
                    .ok_or_else(|| failure("missing ramp entry level"))?,
            )?;
            session.ramp_trace.push(ramp_sample(
                world.current_tick,
                entity.position,
                (x, y),
                cell,
            ));
            if exit {
                session.exit_level = Some(cell.cliff_level.0);
                return Ok(Some(id));
            }
        }
        _ => {}
    }
    Ok(None)
}

fn ramp_sample(
    tick: u32,
    position: SimVec2,
    cell: (u32, u32),
    terrain: &sim_map::MapCell,
) -> artifacts::RampSample {
    artifacts::RampSample {
        tick,
        cell: [cell.0, cell.1],
        level: terrain.cliff_level.0,
        ramp: terrain.ramp.is_some(),
        position_raw: [position.x.raw(), position.y.raw()],
    }
}

fn verify_ramp_step(
    prior: &Prior,
    cell: (u32, u32),
    terrain: &sim_map::MapCell,
    entry_level: u8,
) -> Result<bool, CaptureError> {
    if !prior.ramp || prior.cell.0.abs_diff(cell.0) > 1 || prior.cell.1.abs_diff(cell.1) > 1 {
        return Err(failure(
            "tracked traversal left the contiguous sampled ramp run",
        ));
    }
    if terrain.ramp.is_some() {
        return Ok(false);
    }
    if terrain.cliff_level.0 == entry_level {
        return Err(failure(
            "tracked actor returned to its entry elevation instead of crossing the ramp",
        ));
    }
    Ok(true)
}

fn event_matches_contact(
    event: &sim_types::CombatEventVisual,
    victim: ([f32; 3], sim_types::PlayerId),
    source: ([f32; 3], sim_types::PlayerId),
) -> bool {
    use sim_types::CombatEventVisual;
    let (from, impact, team) = match event {
        CombatEventVisual::HitscanTracer { from, to, owner }
        | CombatEventVisual::BeamVisual {
            from, to, owner, ..
        } => (Some(from), to, *owner),
        CombatEventVisual::MeleeSlashVisual { pos, owner, .. } => (None, pos, *owner),
        // Projectile launch and shatter alone do not identify a hostile impact.
        _ => return false,
    };
    team == source.1
        && team != victim.1
        && impact[..2] == victim.0[..2]
        && from.is_none_or(|from| from[..2] == source.0[..2])
}

fn correlated_combat(
    world: &mut World,
    session: &mut Session,
) -> Result<Option<EntityId>, CaptureError> {
    let mut query = world.query::<(&LiveBattleBubble, &BubbleInstance)>();
    let pipeline = world.resource::<client::battle_pipeline::session::BattleSession>();
    let events = world.resource::<Events<client::vfx::CombatVisualEvent>>();
    // Damage is proven at its authoritative tick. Its event and pose are deliberately
    // displayed later, so retain that exact proof rather than correlating with a newer world.
    for proof in &session.combat_proofs {
        let victim = query
            .iter(world)
            .find(|(live, _)| live.entity_id == proof.victim)
            .map(|(_, bubble)| &bubble.0);
        let source = query
            .iter(world)
            .find(|(live, _)| live.entity_id == proof.source)
            .map(|(_, bubble)| &bubble.0);
        if let (Some(victim), Some(source)) = (victim, source) {
            if events.iter_current_update_events().any(|event| {
                displayed_contact(
                    proof,
                    pipeline.frames.presented().tick.0,
                    victim,
                    source,
                    &event.0,
                )
            }) {
                session.combat_source = Some(proof.source);
                session.combat_tick = Some(proof.tick);
                session.combat_generation = Some(proof.generation);
                return Ok(Some(proof.victim));
            }
        }
    }
    session
        .combat_proofs
        .retain(|proof| proof.tick > pipeline.frames.presented().tick.0);
    let simulation = world
        .resource::<client::battle_pipeline::session::BattleSession>()
        .world()
        .ok_or_else(|| failure("battle disappeared"))?;
    let snapshot = pipeline.frames.current();
    if snapshot.tick.0 != simulation.current_tick {
        return Err(failure("combat proof world/output tick mismatch"));
    }
    for (&id, victim) in &simulation.entities {
        if !session
            .prior
            .get(&id)
            .is_some_and(|old| victim.hp < old.hp || victim.shields < old.shields)
        {
            continue;
        }
        let Ok(victim_index) = snapshot
            .bubbles
            .binary_search_by_key(&id, |bubble| bubble.entity_id)
        else {
            continue;
        };
        let output_victim = &snapshot.bubbles[victim_index];
        for (&source_id, source) in &simulation.entities {
            if source.current_target != Some(id)
                || source.owner == victim.owner
                || !source.is_alive()
            {
                continue;
            }
            let Ok(source_index) = snapshot
                .bubbles
                .binary_search_by_key(&source_id, |bubble| bubble.entity_id)
            else {
                continue;
            };
            let output_source = &snapshot.bubbles[source_index];
            session.combat_proofs.push(CombatProof {
                generation: pipeline
                    .commands
                    .generation()
                    .ok_or_else(|| failure("combat proof has no committed generation"))?
                    .value(),
                tick: simulation.current_tick,
                victim: id,
                source: source_id,
                victim_position: output_victim.world_pos,
                source_position: output_source.world_pos,
                victim_owner: output_victim.owner,
                source_owner: output_source.owner,
            });
        }
    }
    Ok(None)
}

fn displayed_contact(
    proof: &CombatProof,
    displayed_tick: u32,
    victim: &sim_types::BubbleRenderInstance,
    source: &sim_types::BubbleRenderInstance,
    event: &sim_types::CombatEventVisual,
) -> bool {
    displayed_tick == proof.tick
        && victim.entity_id == proof.victim
        && source.entity_id == proof.source
        && victim.world_pos.map(f32::to_bits) == proof.victim_position.map(f32::to_bits)
        && source.world_pos.map(f32::to_bits) == proof.source_position.map(f32::to_bits)
        && victim.owner == proof.victim_owner
        && source.owner == proof.source_owner
        && event_matches_contact(
            event,
            (proof.victim_position, proof.victim_owner),
            (proof.source_position, proof.source_owner),
        )
}

fn validate_callback_token(
    stage: Stage,
    expected: Entity,
    received: Entity,
) -> Result<(), CaptureError> {
    if stage != Stage::Readback || expected != received {
        return Err(failure("out-of-order screenshot callback"));
    }
    Ok(())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn readback(
    trigger: Trigger<'_, ScreenshotCaptured>,
    mut session: ResMut<'_, Session>,
    pipeline: Res<'_, client::battle_pipeline::session::BattleSession>,
    visual: super::ownership::ReadbackEvidence<'_, '_>,
    audit: Option<Res<'_, crate::shell_order_audit::AuditBridge>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let result = (|| -> Result<(), CaptureError> {
        let (entity, mut record) = session
            .pending
            .take()
            .ok_or_else(|| failure("unexpected screenshot callback"))?;
        validate_callback_token(session.stage, entity, trigger.entity())?;
        if pipeline
            .world()
            .map(|world| (world.current_tick, world.compute_checksum()))
            != Some((record.tick, record.checksum))
        {
            return Err(failure("simulation changed during asynchronous readback"));
        }
        if let Some(proof) = &record.ownership {
            proof.frozen.verify(&visual.capture()?)?;
        }
        let image = trigger
            .event()
            .0
            .clone()
            .try_into_dynamic()
            .map_err(|error| CaptureError::Readback(error.to_string()))?
            .to_rgb8();
        if [image.width(), image.height()] != session.script.resolution {
            return Err(CaptureError::Readback(
                "screenshot dimensions differ from requested physical resolution".into(),
            ));
        }
        record.pixels = Some(artifacts::measure(&image, session.script.gameplay_region)?);
        image
            .save(&record.image)
            .map_err(|error| CaptureError::Readback(error.to_string()))?;
        if record.goal == Goal::Swarm {
            if let Some(audit) = audit.as_ref() {
                audit.failure()?;
                audit.callback(entity)?;
                session.audit_pending = Some(record);
                session.stage = Stage::AuditDrain;
                return Ok(());
            }
        }
        info!(
            "Verified runtime capture {} at tick {}: {}",
            record.name,
            record.tick,
            record.image.display()
        );
        session.records.push(record);
        session.index += 1;
        session.stage = if session.index == session.script.checkpoints.len() {
            Stage::Complete
        } else {
            Stage::Apply
        };
        session.manifest()?;
        Ok(())
    })();
    if let Err(error) = result {
        error!("{error}");
        session.failure = Some(error.to_string());
        session.stage = Stage::Failed;
        if let Some(audit) = audit.as_ref() {
            if let Err(write_error) = save_audit_incomplete(audit, &session.out) {
                error!("Incomplete shell audit: {write_error}");
            }
        }
        if let Err(error) = session.manifest() {
            error!("Capture failure manifest: {error}");
        }
        exit.send(AppExit::error());
    } else if session.stage == Stage::Complete {
        session
            .completed
            .store(true, std::sync::atomic::Ordering::Release);
        exit.send(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_ownership_focus_requires_four_subsequent_ready_frames(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let path = root.join("captures/player_ownership.ron");
        let script = CaptureScript::load(&path)?;
        let index = script
            .checkpoints
            .iter()
            .position(|point| point.goal == Goal::EffectLaser)
            .ok_or("actual laser checkpoint")?;
        let output = tempfile::tempdir()?;
        let mut session = Session::new(
            script,
            PathBuf::from("scenario.ron"),
            path,
            root,
            output.path().to_owned(),
            client::presentation::AudioPolicy::DisabledNoPlaybackSubsystem,
        );
        session.index = index;
        session.ui_settled = true;
        session.settled = 3;
        for _ in 0..6 {
            assert!(!session.settled_for_verification(true));
            assert_eq!(session.settled, 0);
        }
        session.focus = Some(Vec2::new(2.0, 3.0));
        for frame in 1..=4 {
            assert_eq!(session.settled_for_verification(true), frame == 4);
            assert_eq!(session.settled, frame);
        }
        Ok(())
    }

    #[test]
    fn actual_production_age_precedes_four_frozen_driver_frames(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let path = root.join("captures/player_ownership.ron");
        let script = CaptureScript::load(&path)?;
        let index = script
            .checkpoints
            .iter()
            .position(|point| point.goal == Goal::EffectLightning)
            .ok_or("lightning checkpoint")?;
        let output = tempfile::tempdir()?;
        let mut session = Session::new(
            script,
            PathBuf::from("scenario.ron"),
            path,
            root,
            output.path().to_owned(),
            client::presentation::AudioPolicy::DisabledNoPlaybackSubsystem,
        );
        session.index = index;
        session.ui_settled = true;
        let mut app = super::super::ownership::tests::preparation_app(
            super::super::ownership::EffectKind::Lightning,
        )?;
        let mut frozen = 0;
        let mut pending = 0;
        for _ in 0..40 {
            app.update();
            let (focus, ready) = super::super::ownership::tests::sample_readiness(app.world_mut())?;
            session.focus = focus;
            if ready {
                frozen += 1;
            } else {
                pending += 1;
            }
            assert_eq!(session.settled_for_verification(ready), frozen >= 4);
            assert_eq!(session.settled, frozen);
            if frozen == 4 {
                break;
            }
        }
        assert!(pending > 4);
        assert_eq!(frozen, 4);
        Ok(())
    }

    #[test]
    fn rejected_actual_transport_writes_incomplete_manifest_and_nonzero_exit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for goal in [Goal::CargoLoaded, Goal::CargoUnloaded] {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
            let definition = client::runtime_battle::BattleDefinition::load(
                &root,
                client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref(),
            )?;
            let mut simulation = definition.instantiate()?;
            let spec = super::super::config::TransportCaptureSpec {
                owner: 0,
                passenger_ordinal: 2,
                carrier_ordinal: 3,
                destination: [12, 10],
            };
            let mut transport =
                super::super::transport::TransportState::select(&simulation, &spec)?;
            transport.apply(&mut simulation, Goal::CargoLoaded)?;
            let script_path = root.join("captures/default_battle.ron");
            let mut script = CaptureScript::load(&script_path)?;
            script.transport = Some(spec);
            script.checkpoints = [Goal::CargoBefore, Goal::CargoLoaded, Goal::CargoUnloaded]
                .into_iter()
                .enumerate()
                .map(|(index, goal)| super::super::config::Checkpoint {
                    name: format!("cargo{index}"),
                    goal,
                    distance: 15.0,
                    max_ticks: 1,
                })
                .collect();
            script.validate()?;
            let output = tempfile::tempdir()?;
            let mut session = Session::new(
                script,
                definition.source,
                script_path,
                root,
                output.path().to_owned(),
                client::presentation::AudioPolicy::DisabledNoPlaybackSubsystem,
            );
            session.transport = Some(transport);
            session.index = if goal == Goal::CargoLoaded { 1 } else { 2 };
            session.stage = Stage::Apply;
            let mut world = World::new();
            world.insert_resource(LabState::default());
            world.insert_resource(
                client::battle_pipeline::session::BattleSession::default().with_world(simulation),
            );
            world.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
            world.init_resource::<Events<AppExit>>();
            let error = control_inner(&mut world, &mut session)
                .err()
                .ok_or("duplicate load or occupied unload must fail")?;
            session.fail(error, &mut world);
            assert!(session.records.is_empty());
            assert!(session.stage == Stage::Failed);
            let manifest = std::fs::read_to_string(output.path().join("manifest.ron"))?;
            assert!(manifest.contains("complete: false"));
            assert!(manifest.contains("real transport transaction rejected"));
            assert!(world
                .resource::<Events<AppExit>>()
                .iter_current_update_events()
                .any(|exit| !exit.is_success()));
        }
        Ok(())
    }

    #[test]
    fn delayed_combat_requires_original_tick_ids_positions_and_event(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut simulation = sim_fixtures::default_battle_sandbox()?;
        let snapshot = simulation.build_render_snapshot();
        let victim = snapshot.bubbles.first().ok_or("victim absent")?;
        let source = snapshot
            .bubbles
            .iter()
            .find(|bubble| bubble.owner != victim.owner)
            .ok_or("source absent")?;
        let proof = CombatProof {
            generation: 1,
            tick: 17,
            victim: victim.entity_id,
            source: source.entity_id,
            victim_position: victim.world_pos,
            source_position: source.world_pos,
            victim_owner: victim.owner,
            source_owner: source.owner,
        };
        let event = sim_types::CombatEventVisual::HitscanTracer {
            from: source.world_pos,
            to: victim.world_pos,
            owner: source.owner,
        };
        assert!(!displayed_contact(&proof, 16, victim, source, &event));
        assert!(displayed_contact(&proof, 17, victim, source, &event));
        assert!(!displayed_contact(&proof, 18, victim, source, &event));
        let mut displaced = victim.clone();
        displaced.world_pos[0] += 0.5;
        assert!(!displayed_contact(&proof, 17, &displaced, source, &event));
        let mut reused = victim.clone();
        reused.entity_id = source.entity_id;
        assert!(!displayed_contact(&proof, 17, &reused, source, &event));
        Ok(())
    }

    #[test]
    fn stale_or_unsolicited_readback_tokens_are_rejected() {
        let expected = Entity::from_raw(4);
        assert!(validate_callback_token(Stage::Readback, expected, expected).is_ok());
        assert!(validate_callback_token(Stage::Readback, expected, Entity::from_raw(5)).is_err());
        for stage in [Stage::Apply, Stage::Settle, Stage::Complete, Stage::Failed] {
            assert!(validate_callback_token(stage, expected, expected).is_err());
        }
    }

    #[test]
    fn bounded_waits_fail_without_a_virtual_clock_or_renderer(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for wall_deadline in [true, false] {
            let output = tempfile::tempdir()?;
            let script_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/captures/default_battle.ron");
            let script = CaptureScript::load(&script_path)?;
            let mut session = Session::new(
                script.clone(),
                PathBuf::from("scenario.ron"),
                script_path,
                PathBuf::from("assets"),
                output.path().to_owned(),
                client::presentation::AudioPolicy::DisabledNoPlaybackSubsystem,
            );
            if wall_deadline {
                session.deadline = Instant::now()
                    .checked_sub(Duration::from_secs(script.timeout_seconds + 1))
                    .ok_or("monotonic test clock underflow")?;
            } else {
                session.frames = script.max_frames;
            }
            let mut world = World::new();
            world.insert_resource(session);
            world.init_resource::<Events<AppExit>>();
            observe(&mut world);
            assert!(world.resource::<Session>().stage == Stage::Failed);
            let manifest = std::fs::read_to_string(output.path().join("manifest.ron"))?;
            assert!(manifest.contains("complete: false"));
            assert!(manifest.contains("timed out"));
            assert!(world
                .resource::<Events<AppExit>>()
                .iter_current_update_events()
                .any(|exit| !exit.is_success()));
        }
        Ok(())
    }

    #[test]
    fn combat_contact_rejects_unrelated_impact_origin_and_friendly_event() {
        use sim_types::{CombatEventVisual, PlayerId};
        let source = ([1.0, 2.0, 0.0], PlayerId(0));
        let victim = ([4.0, 5.0, 0.0], PlayerId(1));
        let tracer = |from, to, owner| CombatEventVisual::HitscanTracer { from, to, owner };
        assert!(event_matches_contact(
            &tracer(source.0, victim.0, source.1),
            victim,
            source
        ));
        assert!(!event_matches_contact(
            &tracer(source.0, [4.0, 6.0, 0.0], source.1),
            victim,
            source
        ));
        assert!(!event_matches_contact(
            &tracer([1.0, 3.0, 0.0], victim.0, source.1),
            victim,
            source
        ));
        assert!(!event_matches_contact(
            &tracer(source.0, victim.0, victim.1),
            victim,
            source
        ));
        assert!(!event_matches_contact(
            &CombatEventVisual::BubbleShatter {
                pos: victim.0,
                radius: 1.0,
                is_dome: false,
                owner: victim.1
            },
            victim,
            source
        ));
        assert!(!event_matches_contact(
            &CombatEventVisual::ProjectileVisual {
                pos: victim.0,
                vel: [1.0; 3],
                owner: source.1,
                duration_secs: None
            },
            victim,
            source
        ));
    }

    #[test]
    fn ramp_proof_requires_uninterrupted_adjacent_samples_and_opposite_landing(
    ) -> Result<(), CaptureError> {
        let mut prior = Prior {
            position: SimVec2::ZERO,
            hp: SimScalar::ONE,
            shields: SimScalar::ZERO,
            level: 1,
            ramp: true,
            cell: (2, 3),
        };
        let mut ramp = sim_map::MapCell::default_ground(sim_map::CliffLevel(1));
        ramp.ramp = Some(sim_map::RampDir::East);
        assert!(!verify_ramp_step(&prior, (3, 3), &ramp, 1)?);
        let landing = sim_map::MapCell::default_ground(sim_map::CliffLevel(2));
        assert!(verify_ramp_step(&prior, (3, 3), &landing, 1)?);
        assert!(verify_ramp_step(&prior, (7, 3), &landing, 1).is_err());
        assert!(verify_ramp_step(&prior, (3, 3), &landing, 2).is_err());
        prior.ramp = false;
        assert!(verify_ramp_step(&prior, (3, 3), &landing, 1).is_err());
        Ok(())
    }
}
