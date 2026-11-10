//! The effect lab's frame-by-frame driver: warm-up, takes, exact-age captures.
//!
//! # Responsibility
//! Owns [`LabDriver`], the state machine that walks a [`LabProgram`]: it waits until the
//! stage's textures are loaded and the render world has no pipeline left to compile (the
//! subject drawn meanwhile so every effect pipeline is specialized), then for each take resets
//! the effect runtime, poses the battle camera, lays the take's ground, spawns the subject and
//! steps `Time<Virtual>` in steps of at most the take's step until each shot's exact age, where
//! it draws one settling frame, then requests a screenshot and holds time still until the PNG
//! is written.
//!
//! # How it fits
//! Time advances only through [`TimeUpdateStrategy::ManualDuration`], set here each frame for
//! the next frame's time update, so an effect's picture at an age is the same on every run of a
//! driver: the layers are pure functions of age, and particles integrate the same steps.
//!
//! # Invariants
//! - No step crosses a shot's age; while a capture is pending the step is zero.
//! - A failed capture, a missing effect or a failed texture ends the app with an error.

use super::program::{next_step, LabProgram, LabTake};
use super::scene::{material_of, LabScene};
use crate::effects::catalog::EffectCatalog;
use crate::effects::placement::WorldLine;
use crate::effects::runtime::{EffectRuntime, SpawnRequest};
use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
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
        spawn_subject(program, catalog, runtime, now, 0.0)?;
    }
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
    world.commands.spawn(Screenshot::primary_window()).observe(
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
    let zoom = take.map_or(super::program::LabZoom::Default, |take| take.zoom);
    if let Ok(mut transform) = transforms.get_mut(camera) {
        *transform = zoom.camera(program.subject.focus()).compute_transform();
    }
}

/// Spawns the subject `delay` seconds from `now` (negative: born in the past).
fn spawn_subject(
    program: &LabProgram,
    catalog: &EffectCatalog,
    runtime: &mut EffectRuntime,
    now: f64,
    delay: f32,
) -> Result<(), String> {
    let subject = &program.subject;
    let effect = catalog.effect_id(&subject.effect).ok_or_else(|| {
        format!(
            "no effect named {:?} in {}",
            subject.effect,
            catalog.path.display()
        )
    })?;
    let anchor = Vec3::new(subject.anchor.x, 0.0, subject.anchor.z);
    let line = subject.line_to.map(|to| WorldLine {
        from: anchor,
        to: Vec3::new(to.x, 0.0, to.z),
    });
    let request = SpawnRequest {
        effect,
        anchor,
        bound: subject.bound,
        line,
        delay,
    };
    runtime
        .spawn(catalog, &request, now)
        .map(|_| ())
        .ok_or_else(|| "the effect pool refused the subject".into())
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
