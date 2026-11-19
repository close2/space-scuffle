//! The effect lab's frame-by-frame driver: warm-up, takes, exact-age captures.
//!
//! # Responsibility
//! Owns [`LabDriver`], the state machine that walks a [`LabProgram`]: it waits until the
//! stage's textures are loaded and the render world has no pipeline left to compile (the
//! subject drawn meanwhile so every effect pipeline is specialized), then for each take resets
//! the effect runtime, poses the battle camera, lays the take's ground, spawns the subject and
//! steps `Time<Virtual>` in steps of at most the take's step until each shot's exact age, where
//! it draws one settling frame, then requests a screenshot of the lab's [`CaptureTarget`] (the
//! window or the offscreen canvas) and holds time still until the PNG is written.
//!
//! # How it fits
//! Time advances only through [`TimeUpdateStrategy::ManualDuration`], set here each frame for
//! the next frame's time update, so an effect's picture at an age is the same on every run of a
//! driver: the layers are pure functions of age, and particles integrate the same steps.
//!
//! # Invariants
//! - No step crosses a shot's age; while a capture is pending the step is zero.
//! - A failed capture, a missing effect or a failed texture ends the app with an error.

use super::program::{next_step, LabCrowd, LabCrowdMember, LabProgram, LabSubject, LabTake};
use super::scene::{material_of, LabScene};
use crate::effects::catalog::schema::EventKind;
use crate::effects::catalog::EffectCatalog;
use crate::effects::placement::WorldLine;
use crate::effects::pool::EffectHandle;
use crate::effects::runtime::{EffectRuntime, SpawnRequest};
use crate::offscreen::CaptureTarget;
use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::view::screenshot::ScreenshotCaptured;
use bevy::time::TimeUpdateStrategy;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Frames the warm-up lasts at least.
const WARM_FRAMES: u32 = 12;
/// Consecutive render frames with no pipeline waiting before the first take.
const IDLE_FRAMES: u32 = 8;
/// Warm-up frames after which a lab that never settles fails.
const MAX_WARM_FRAMES: u32 = 3000;
/// Age at which the subject is shown during the warm-up, seconds (most layers are on).
const WARM_AGE: f32 = 0.25;

/// Consecutive render frames in which no pipeline was waiting to compile, shared with the
/// render world.
#[derive(Resource, Debug, Clone, Default)]
pub struct LabRenderIdle(pub Arc<AtomicU32>);

/// Render-world system: counts frames without a waiting pipeline.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn track_render_idle(cache: Res<'_, PipelineCache>, idle: Res<'_, LabRenderIdle>) {
    if cache.waiting_pipelines().next().is_none() {
        idle.0.fetch_add(1, Ordering::AcqRel);
    } else {
        idle.0.store(0, Ordering::Release);
    }
}

/// Where the driver is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LabPhase {
    /// Waiting for the vocabulary and the stage.
    Loading,
    /// The subject is shown while textures load and pipelines compile.
    Warming {
        /// Frames so far.
        frames: u32,
    },
    /// Take `0` starts this frame.
    Start(usize),
    /// A take runs.
    Running {
        /// Index of the take.
        take: usize,
        /// Index of its next shot.
        shot: usize,
        /// `Time<Virtual>` of the take's spawn, seconds.
        spawned: f64,
    },
    /// Every take is captured.
    Done,
}

/// The driver's state.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct LabDriver {
    /// Phase.
    pub phase: LabPhase,
    /// A screenshot was requested and is not written yet.
    pub pending: bool,
    /// Whether this frame draws the reference overlay.
    pub overlay: bool,
    /// The first failure, which ends the app.
    pub failed: Option<String>,
    /// PNGs written.
    pub written: u32,
    /// The current shot's state has been drawn once and the screenshot is due.
    pub settled: bool,
}

impl Default for LabDriver {
    fn default() -> Self {
        Self {
            phase: LabPhase::Loading,
            pending: false,
            overlay: false,
            failed: None,
            written: 0,
            settled: false,
        }
    }
}

/// The live handles of the take's [`LabCrowd`], by member: `None` before a member is spawned,
/// once it was refused, or after it was released and gone.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct LabCrowdState {
    /// Per member: whether it was spawned this take, and its handle.
    members: Vec<(bool, Option<EffectHandle>)>,
    /// Per member: whether it was released this take.
    released: Vec<bool>,
}

/// The resources and queries the driver changes.
#[derive(bevy::ecs::system::SystemParam)]
pub struct LabWorld<'w, 's> {
    /// Commands for screenshots.
    commands: Commands<'w, 's>,
    /// The vocabulary.
    catalog: Option<Res<'w, EffectCatalog>>,
    /// The live effects.
    runtime: Option<ResMut<'w, EffectRuntime>>,
    /// The stage.
    scene: Option<Res<'w, LabScene>>,
    /// The clock.
    time: Res<'w, Time<Virtual>>,
    /// The next time step.
    strategy: ResMut<'w, TimeUpdateStrategy>,
    /// Texture loading.
    server: Res<'w, AssetServer>,
    /// Render readiness.
    idle: Res<'w, LabRenderIdle>,
    /// Camera and ground poses and materials.
    transforms: Query<'w, 's, &'static mut Transform>,
    /// The ground's material.
    grounds: Query<'w, 's, &'static mut MeshMaterial3d<StandardMaterial>>,
    /// The app's exit.
    exit: EventWriter<'w, AppExit>,
    /// What a screenshot copies: the window or the offscreen canvas.
    capture: Res<'w, CaptureTarget>,
    /// The take's further effects, if the program has any, and their handles.
    crowd: (Option<Res<'w, LabCrowd>>, ResMut<'w, LabCrowdState>),
}

/// Advances the program by one frame.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn drive_lab(
    mut driver: ResMut<'_, LabDriver>,
    program: Res<'_, LabProgram>,
    mut world: LabWorld<'_, '_>,
) {
    driver.overlay = false;
    *world.strategy = TimeUpdateStrategy::ManualDuration(Duration::ZERO);
    if let Some(failure) = driver.failed.take() {
        error!("Effect lab failed: {failure}");
        world.exit.send(AppExit::error());
        driver.phase = LabPhase::Done;
        return;
    }
    if driver.pending || driver.phase == LabPhase::Done {
        return;
    }
    let result = match driver.phase {
        LabPhase::Loading => load(&mut driver, &program, &mut world),
        LabPhase::Warming { frames } => warm(&mut driver, frames, &mut world),
        LabPhase::Start(take) => start(&mut driver, &program, take, &mut world),
        LabPhase::Running {
            take,
            shot,
            spawned,
        } => {
            advance(&mut driver, &program, (take, shot, spawned), &mut world);
            Ok(())
        }
        LabPhase::Done => Ok(()),
    };
    if let Err(failure) = result {
        driver.failed = Some(failure);
    }
}

/// Shows the subject at the warm-up age once the vocabulary and the stage exist.
fn load(
    driver: &mut LabDriver,
    program: &LabProgram,
    world: &mut LabWorld<'_, '_>,
) -> Result<(), String> {
    let (Some(catalog), Some(runtime), Some(scene)) = (
        world.catalog.as_deref(),
        world.runtime.as_deref_mut(),
        world.scene.as_deref(),
    ) else {
        return Ok(());
    };
    let first = program.takes.first();
    pose(scene.camera, first, program, &mut world.transforms);
    runtime.reset();
    spawn_subject(
        program,
        catalog,
        runtime,
        world.time.elapsed_secs_f64(),
        -WARM_AGE,
    )?;
    driver.overlay = true;
    driver.phase = LabPhase::Warming { frames: 0 };
    Ok(())
}

/// Waits for textures and pipelines.
fn warm(driver: &mut LabDriver, frames: u32, world: &mut LabWorld<'_, '_>) -> Result<(), String> {
    driver.overlay = true;
    let Some(scene) = world.scene.as_deref() else {
        return Err("the lab stage disappeared".into());
    };
    let mut loaded = true;
    for image in &scene.images {
        match world.server.get_load_state(image.id()) {
            Some(LoadState::Failed(error)) => return Err(format!("a ground texture: {error}")),
            Some(LoadState::Loaded) => {}
            _ => loaded = false,
        }
    }
    let idle = world.idle.0.load(Ordering::Acquire);
    if loaded && frames >= WARM_FRAMES && idle >= IDLE_FRAMES {
        driver.phase = LabPhase::Start(0);
        return Ok(());
    }
    if frames >= MAX_WARM_FRAMES {
        return Err(format!(
            "the renderer never settled (textures loaded: {loaded}, idle frames: {idle})"
        ));
    }
    driver.phase = LabPhase::Warming { frames: frames + 1 };
    Ok(())
}

/// Starts take `index`: camera, ground, a fresh runtime and the subject's spawn.
fn start(
    driver: &mut LabDriver,
    program: &LabProgram,
    index: usize,
    world: &mut LabWorld<'_, '_>,
) -> Result<(), String> {
    let Some(take) = program.takes.get(index) else {
        driver.phase = LabPhase::Done;
        world.exit.send(AppExit::Success);
        return Ok(());
    };
    let (Some(catalog), Some(runtime), Some(scene)) = (
        world.catalog.as_deref(),
        world.runtime.as_deref_mut(),
        world.scene.as_deref(),
    ) else {
        return Err("the vocabulary or the stage disappeared".into());
    };
    pose(scene.camera, Some(take), program, &mut world.transforms);
    if let Ok(mut material) = world.grounds.get_mut(scene.ground) {
        material.0 = material_of(scene, take.ground);
    }
    runtime.reset();
    runtime.only = take.only;
    let now = world.time.elapsed_secs_f64();
    if take.effect {
        match program.subject.repeat {
            Some(repeat) => {
                for delay in repeat.delays() {
                    spawn_subject(program, catalog, runtime, now, delay)?;
                }
            }
            None => spawn_subject(program, catalog, runtime, now, 0.0)?,
        }
    }
    let members = world
        .crowd
        .0
        .as_ref()
        .map_or(0, |crowd| crowd.members.len());
    let state = &mut *world.crowd.1;
    state.members.clear();
    state.members.resize(members, (false, None));
    state.released.clear();
    state.released.resize(members, false);
    driver.phase = LabPhase::Running {
        take: index,
        shot: 0,
        spawned: now,
    };
    advance(driver, program, (index, 0, now), world);
    Ok(())
}

/// Steps towards the take's next shot or captures it.
fn advance(
    driver: &mut LabDriver,
    program: &LabProgram,
    (index, shot, spawned): (usize, usize, f64),
    world: &mut LabWorld<'_, '_>,
) {
    let Some(take) = program.takes.get(index) else {
        driver.phase = LabPhase::Start(index);
        return;
    };
    let Some(next) = take.shots.get(shot) else {
        driver.phase = LabPhase::Start(index + 1);
        return;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a take's age in seconds fits f32"
    )]
    let age = (world.time.elapsed_secs_f64() - spawned) as f32;
    if let Err(failure) = keep_crowd(world, (take, spawned, age), program) {
        driver.failed = Some(failure);
        return;
    }
    let step = next_step(age, next.age, take.step);
    if step > 0.0 {
        *world.strategy = TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(step));
        return;
    }
    driver.overlay = next.overlay;
    if !driver.settled {
        // One frame at the shot's age and overlay before the screenshot, so the captured frame
        // is that state whichever frame the readback copies.
        driver.settled = true;
        return;
    }
    driver.settled = false;
    driver.pending = true;
    let path = program.out.join(format!("{}.png", next.name));
    world.commands.spawn(world.capture.screenshot()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>, mut driver: ResMut<'_, LabDriver>| {
            let written = trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .map_err(|error| error.to_string())
                .and_then(|image| write_png(&image.to_rgb8(), &path));
            match written {
                Ok(()) => driver.written += 1,
                Err(error) => driver.failed = Some(format!("{}: {error}", path.display())),
            }
            driver.pending = false;
        },
    );
    driver.phase = LabPhase::Running {
        take: index,
        shot: shot + 1,
        spawned,
    };
}

/// Poses the camera for `take` (the first take's zoom before any take).
fn pose(
    camera: Entity,
    take: Option<&LabTake>,
    program: &LabProgram,
    transforms: &mut Query<'_, '_, &'static mut Transform>,
) {
    pose_at(camera, take, program, 1.0, transforms);
}

/// Poses the camera for `take` with its distance scaled by `scale`.
fn pose_at(
    camera: Entity,
    take: Option<&LabTake>,
    program: &LabProgram,
    scale: f32,
    transforms: &mut Query<'_, '_, &'static mut Transform>,
) {
    let zoom = take.map_or(super::program::LabZoom::Default, |take| take.zoom);
    if let Ok(mut transform) = transforms.get_mut(camera) {
        let mut rts = zoom.camera(program.subject.focus());
        rts.distance *= scale;
        *transform = rts.compute_transform();
    }
}

/// Keeps the take's crowd at `age`: spawns or holds the members due by now (born at their own
/// age), releases those due, moves the drifting ones, sets the sweeping fog clips, and steps
/// the camera outward by the crowd's dolly. A program without a crowd does nothing.
fn keep_crowd(
    world: &mut LabWorld<'_, '_>,
    (take, spawned, age): (&LabTake, f64, f32),
    program: &LabProgram,
) -> Result<(), String> {
    let Some(crowd) = world.crowd.0.as_deref() else {
        return Ok(());
    };
    let (Some(catalog), Some(runtime), Some(scene)) = (
        world.catalog.as_deref(),
        world.runtime.as_deref_mut(),
        world.scene.as_deref(),
    ) else {
        return Err("the vocabulary or the stage disappeared".into());
    };
    if crowd.dolly_per_second != 0.0 {
        let scale = 1.0 + crowd.dolly_per_second * age;
        pose_at(
            scene.camera,
            Some(take),
            program,
            scale,
            &mut world.transforms,
        );
    }
    let now = world.time.elapsed_secs_f64();
    let state = &mut *world.crowd.1;
    if !take.effect {
        return Ok(());
    }
    for (index, member) in crowd.members.iter().enumerate() {
        let (Some(spawned_flag), Some(released)) =
            (state.members.get_mut(index), state.released.get_mut(index))
        else {
            break;
        };
        if !spawned_flag.0 && age >= member.at {
            spawned_flag.0 = true;
            let born = spawned + f64::from(member.at);
            spawned_flag.1 = spawn_member(member, catalog, runtime, born, now)?;
        }
        let Some(handle) = spawned_flag.1 else {
            continue;
        };
        if let Some(release_at) = member.release_at.filter(|at| age >= *at && !*released) {
            *released = true;
            runtime.release(handle, spawned + f64::from(release_at));
        }
        keep_member(member, handle, runtime, age);
    }
    Ok(())
}

/// Spawns (or holds) `member`, born at `born` (seconds of `Time<Virtual>`, not after `now`).
fn spawn_member(
    member: &LabCrowdMember,
    catalog: &EffectCatalog,
    runtime: &mut EffectRuntime,
    born: f64,
    now: f64,
) -> Result<Option<EffectHandle>, String> {
    let request = lab_request(&member.subject, catalog, 0.0)?;
    if member.held {
        return Ok(runtime.hold(catalog, &request, born, false));
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a frame's lateness in seconds fits f32"
    )]
    let delay = (born - now) as f32;
    Ok(runtime.spawn(catalog, &SpawnRequest { delay, ..request }, now))
}

/// Moves `member`'s live effect `handle` to where it is at `age` and sets its fog clip.
fn keep_member(
    member: &LabCrowdMember,
    handle: EffectHandle,
    runtime: &mut EffectRuntime,
    age: f32,
) {
    if member.velocity != Vec3::ZERO {
        let anchor = member.anchor_at(age);
        let anchor = Vec3::new(anchor.x, member.subject.anchor.y, anchor.z);
        match (member.held, member.line_to_at(age)) {
            (true, Some(to)) => {
                let line = WorldLine {
                    seen: member.subject.line_seen,
                    ..WorldLine::whole(anchor, Vec3::new(to.x, anchor.y, to.z))
                };
                runtime.update_held_line(handle, line, member.subject.bound);
            }
            (true, None) => {
                runtime.update_held(handle, anchor, member.subject.bound);
            }
            (false, _) => {
                runtime.move_anchor(handle, anchor);
            }
        }
    }
    if let Some(sweep) = member.fog_sweep {
        runtime.set_fog_clip(handle, sweep.clip_at(age));
    }
}

/// Height above the ground, world units, at which the simulation publishes a shot's ends and
/// its impact (`sim_core`'s combat writes them half a unit up), and so the lab spawns them.
pub const SHOT_HEIGHT: f32 = 0.5;

/// Spawns the subject `delay` seconds from `now` (negative: born in the past).
fn spawn_subject(
    program: &LabProgram,
    catalog: &EffectCatalog,
    runtime: &mut EffectRuntime,
    now: f64,
    delay: f32,
) -> Result<(), String> {
    let subject = &program.subject;
    let request = lab_request(subject, catalog, delay)?;
    let (effect, line) = (request.effect, request.line);
    let kind = catalog.effect(effect).map(|effect| effect.event);
    let handle = runtime
        .spawn(catalog, &request, now)
        .ok_or_else(|| "the effect pool refused the subject".to_owned())?;
    // A shot under the lab's fog is cut where its path leaves the seen ground, as the event
    // wiring cuts it in a battle (plan WP11).
    if let (Some(EventKind::Shot), Some(below_x), Some(line)) = (kind, subject.fog_below_x, line) {
        runtime.set_fog_clip(
            handle,
            crate::effects::systems::shot_clip(
                line,
                catalog
                    .effect(effect)
                    .map_or(crate::effects::catalog::ShotReach::default(), |effect| {
                        effect.shot_reach
                    }),
                |x, _| x >= below_x,
            ),
        );
    }
    Ok(())
}

/// The spawn request of `subject`, `delay` seconds late: on the ground, or half a unit up for
/// a shot and its impact, as the simulation publishes them.
fn lab_request(
    subject: &LabSubject,
    catalog: &EffectCatalog,
    delay: f32,
) -> Result<SpawnRequest, String> {
    let effect = catalog.effect_id(&subject.effect).ok_or_else(|| {
        format!(
            "no effect named {:?} in {}",
            subject.effect,
            catalog.path.display()
        )
    })?;
    let kind = catalog.effect(effect).map(|effect| effect.event);
    // A shot and its impact are published where the simulation fires them, half a unit above
    // the ground (plan WP11); every other event lies on the ground.
    let height = match kind {
        Some(EventKind::Shot | EventKind::Point) => SHOT_HEIGHT,
        _ => 0.0,
    };
    let anchor = Vec3::new(subject.anchor.x, height, subject.anchor.z);
    let line = subject.line_to.map(|to| WorldLine {
        seen: subject.line_seen,
        ..WorldLine::whole(anchor, Vec3::new(to.x, height, to.z))
    });
    Ok(SpawnRequest {
        effect,
        anchor,
        bound: subject.bound,
        line,
        delay,
    })
}

/// Writes `image` as a PNG with fast compression: a lab run writes hundreds of frames and the
/// default compression dominates its time.
fn write_png(image: &image::RgbImage, path: &std::path::Path) -> Result<(), String> {
    use image::ImageEncoder;
    let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    );
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| error.to_string())
}
