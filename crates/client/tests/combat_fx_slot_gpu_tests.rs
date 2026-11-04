//! Pooled combat effect slots on a real GPU (remediation finding C-17).
//!
//! # Responsibility
//! Proves that a material slot handed to a second effect draws the second effect, and that an
//! effect's later changes reach the GPU, although neither ever modifies the material asset. The
//! asset still holds the first effect's data, so only the render-world upload of
//! `client::vfx::CombatFxInstance` can make the second capture right.
//!
//! # How it fits
//! The pixel test re-executes this test binary on the virtual GPU, as `dome_fill_tests.rs` does. Its
//! child runs Bevy's real renderer with the production `CombatFxPlugin`, the shipped
//! `combat_fx.wgsl` and the shipped palette, on a black stage with one camera and one effect card.
//! It writes one capture per shot and a report of the slots it used; the parent judges them.
//!
//! # Invariants verified
//! - The second effect gets the first effect's slot (the pool reuses it).
//! - Each capture's light is dominated by the colour channel of the effect drawn in it.
//! - Advancing the effect's age to near the end of its life fades the card, so per-frame writes
//!   reach the slot's GPU buffer.

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::player_presentation::{PlayerPalette, PlayerStyle};
use client::render_compatibility::{run_render_app, RenderCompatibilityPlugin};
use client::runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO};
use client::vfx::{
    CombatFxInstance, CombatFxMaterial, CombatFxMaterialPool, CombatFxPlugin, CombatFxUniformGpu,
    CombatLightEffect,
};
use client::ResolvedAssetsRoot;
use serde::{Deserialize, Serialize};
use sim_types::PlayerId;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_COMBAT_FX_CHILD_OUT";
/// Name of the libtest entry that the pixel test launches as its child.
const CHILD_TEST: &str = "combat_fx_slot_child";
/// File the child writes its [`SlotReport`] to.
const REPORT_FILE: &str = "slots.ron";

/// Capture of the first effect.
const FIRST: &str = "first";
/// Capture of the second effect, drawn with the first effect's slot.
const SECOND: &str = "second";
/// Capture of the second effect after its age is advanced to [`FADED_AGE`].
const FADED: &str = "faded";

/// Frames between a change on the stage and its capture.
const SETTLE_FRAMES: u32 = 4;
/// Captures of the first effect allowed while its pipeline compiles, before the child gives up.
const MAX_FIRST_ATTEMPTS: u32 = 120;
/// Shader kind of the card: the impact starburst, a disc lit mostly in the owner's colour.
const STARBURST_KIND: u32 = 5;
/// Age of a drawn effect. The starburst's light falls as `(1 - age)^2`.
const DRAWN_AGE: f32 = 0.3;
/// Age that fades a starburst below the shader's alpha cut-off.
const FADED_AGE: f32 = 0.97;
/// World size of the effect card, seen by a camera [`CAMERA_DISTANCE`] away.
const CARD_SIZE: f32 = 2.5;
/// Distance from the camera to the card, in world units.
const CAMERA_DISTANCE: f32 = 4.0;

/// Smallest share of a capture's light that its dominant channel must exceed the next one by.
const DOMINANCE_MARGIN: f64 = 0.05;
/// Largest share of the drawn capture's light the faded capture may keep.
const FADED_LIGHT_LIMIT: f64 = 0.02;

/// What the child did, for the parent to check.
#[derive(Debug, Serialize, Deserialize)]
struct SlotReport {
    /// Debug form of the first effect's material asset ID.
    first_slot: String,
    /// Debug form of the second effect's material asset ID.
    second_slot: String,
    /// Dominant linear colour channel (0 red, 1 green, 2 blue) of the first effect's owner.
    first_channel: usize,
    /// Dominant linear colour channel of the second effect's owner.
    second_channel: usize,
    /// Material assets the pool created over the run.
    created: u64,
}

/// Index of the largest of three values.
fn dominant(channels: [f64; 3]) -> usize {
    (0..3)
        .max_by(|a, b| channels[*a].total_cmp(&channels[*b]))
        .expect("invariant: three channels")
}

/// Shot the child is working on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shot {
    First,
    Second,
    Faded,
    Done,
}

impl Shot {
    fn name(self) -> &'static str {
        match self {
            Self::First => FIRST,
            Self::Second => SECOND,
            Self::Faded | Self::Done => FADED,
        }
    }
}

/// The child's stage.
#[derive(Resource)]
struct Stage {
    directory: PathBuf,
    styles: [PlayerStyle; 2],
    channels: [usize; 2],
    quad: Option<Handle<Mesh>>,
    shot: Shot,
    effect: Option<Entity>,
    slots: [Option<AssetId<CombatFxMaterial>>; 2],
    settled_frames: u32,
    first_attempts: u32,
    capture_pending: bool,
    /// Set by the screenshot observer: whether the capture just written showed any light.
    captured: Option<bool>,
}

/// Uniform of an effect card drawn in `style` at `age`.
fn card_uniform(style: PlayerStyle, age: f32) -> CombatFxUniformGpu {
    CombatFxUniformGpu {
        primary_color: style.linear.truncate().extend(0.0),
        age,
        fx_kind: STARBURST_KIND,
        seed: 0.5,
        aspect: 1.0,
    }
}

/// Two shipped preview owners whose colours have different dominant channels.
fn two_owners(palette: &PlayerPalette) -> ([PlayerStyle; 2], [usize; 2]) {
    let styles: Vec<(PlayerStyle, usize)> = (0..=u8::MAX)
        .map(PlayerId)
        .filter(|owner| *owner != PlayerId::NEUTRAL)
        .filter_map(|owner| palette.preview_style(owner).ok())
        .map(|style| {
            let linear = style.linear.truncate().as_dvec3().to_array();
            (style, dominant(linear))
        })
        .collect();
    let (first, first_channel) = *styles
        .first()
        .expect("the shipped palette has a preview owner");
    let (second, second_channel) = *styles
        .iter()
        .find(|(_, channel)| *channel != first_channel)
        .expect("the shipped palette has two preview owners of different dominant colour");
    ([first, second], [first_channel, second_channel])
}

/// Spawns the camera and the card mesh.
fn setup_stage(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut stage: ResMut<'_, Stage>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, CAMERA_DISTANCE).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    stage.quad = Some(meshes.add(Rectangle::new(1.0, 1.0)));
}

/// Spawns one effect card through the pool, as the production spawner does.
fn spawn_card(
    commands: &mut Commands<'_, '_>,
    pool: &mut CombatFxMaterialPool,
    materials: &mut Assets<CombatFxMaterial>,
    quad: Handle<Mesh>,
    style: PlayerStyle,
) -> (Entity, AssetId<CombatFxMaterial>) {
    let (instance, material) = pool.effect(materials, card_uniform(style, DRAWN_AGE));
    let slot = material.0.id();
    let entity = commands
        .spawn((
            CombatLightEffect,
            instance,
            material,
            Mesh3d(quad),
            Transform::from_scale(Vec3::splat(CARD_SIZE)),
        ))
        .id();
    (entity, slot)
}

/// Advances the stage by one frame: acts on the last capture, sets up the shot, and captures it.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn run_shots(
    mut commands: Commands<'_, '_>,
    mut stage: ResMut<'_, Stage>,
    mut pool: ResMut<'_, CombatFxMaterialPool>,
    mut materials: ResMut<'_, Assets<CombatFxMaterial>>,
    mut instances: Query<'_, '_, &mut CombatFxInstance>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let Some(quad) = stage.quad.clone() else {
        return;
    };
    if stage.capture_pending {
        return;
    }
    if let Some(lit) = stage.captured.take() {
        stage.settled_frames = 0;
        match stage.shot {
            Shot::First if !lit => {
                // The pipeline is still compiling; take the shot again.
                stage.first_attempts += 1;
                assert!(
                    stage.first_attempts < MAX_FIRST_ATTEMPTS,
                    "the first effect never drew"
                );
                return;
            }
            Shot::First => {
                let effect = stage
                    .effect
                    .take()
                    .expect("invariant: first effect spawned");
                commands.entity(effect).despawn();
                stage.shot = Shot::Second;
                return;
            }
            Shot::Second => {
                let effect = stage.effect.expect("invariant: second effect spawned");
                let mut instance = instances
                    .get_mut(effect)
                    .expect("invariant: the effect has its instance");
                CombatFxInstance::set_age(&mut instance, FADED_AGE);
                stage.shot = Shot::Faded;
                return;
            }
            Shot::Faded | Shot::Done => {
                stage.shot = Shot::Done;
            }
        }
    }
    if stage.shot == Shot::Done {
        let report = SlotReport {
            first_slot: format!("{:?}", stage.slots[0].expect("invariant: first shot ran")),
            second_slot: format!("{:?}", stage.slots[1].expect("invariant: second shot ran")),
            first_channel: stage.channels[0],
            second_channel: stage.channels[1],
            created: pool.created(),
        };
        let text = ron::to_string(&report).expect("invariant: the report is plain data");
        std::fs::write(stage.directory.join(REPORT_FILE), text).expect("the report is written");
        exit.send(AppExit::Success);
        return;
    }
    if stage.effect.is_none() {
        let index = usize::from(stage.shot == Shot::Second);
        let (entity, slot) = spawn_card(
            &mut commands,
            &mut pool,
            &mut materials,
            quad,
            stage.styles[index],
        );
        stage.effect = Some(entity);
        stage.slots[index] = Some(slot);
    }
    stage.settled_frames += 1;
    if stage.settled_frames < SETTLE_FRAMES {
        return;
    }
    stage.capture_pending = true;
    let path = stage.directory.join(format!("{}.png", stage.shot.name()));
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>, mut stage: ResMut<'_, Stage>| {
            let image = trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgb8();
            image.save(&path).expect("a screenshot must be written");
            stage.captured = Some(image.pixels().any(|pixel| pixel.0 != [0, 0, 0]));
            stage.capture_pending = false;
        },
    );
}

/// Sum of each colour channel over a capture, in levels of 255.
fn light(directory: &Path, name: &str) -> TestResult<[f64; 3]> {
    let image = image::open(directory.join(format!("{name}.png")))?.to_rgb8();
    let mut sums = [0.0; 3];
    for pixel in image.pixels() {
        for (sum, level) in sums.iter_mut().zip(pixel.0) {
            *sum += f64::from(level);
        }
    }
    Ok(sums)
}

/// Asserts that channel `expected` carries the most light of `sums`, by [`DOMINANCE_MARGIN`].
fn assert_dominated_by(name: &str, sums: [f64; 3], expected: usize) {
    let total: f64 = sums.iter().sum();
    assert!(total > 0.0, "{name}: the capture is black");
    let runner_up = (0..3)
        .filter(|channel| *channel != expected)
        .map(|channel| sums[channel])
        .fold(0.0, f64::max);
    assert!(
        (sums[expected] - runner_up) / total > DOMINANCE_MARGIN,
        "{name}: channel {expected} must dominate the light {sums:?}"
    );
}

/// Runs the child and returns the directory holding its captures and report.
fn run_child() -> TestResult<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.assert_passes_validation("combat effect slot child");
    Ok(directory)
}

/// C-17: a reused slot draws its new effect, and an effect's age reaches the GPU every frame.
#[test]
fn a_reused_slot_draws_its_new_effect() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = run_child()?;
    let report: SlotReport = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    assert_eq!(
        report.first_slot, report.second_slot,
        "the second effect must reuse the first effect's slot"
    );
    assert_eq!(report.created, 1, "one material serves both effects");
    let first = light(directory.path(), FIRST)?;
    let second = light(directory.path(), SECOND)?;
    let faded = light(directory.path(), FADED)?;
    eprintln!("light by channel: first {first:?}, second {second:?}, faded {faded:?}");
    assert_dominated_by(FIRST, first, report.first_channel);
    assert_dominated_by(SECOND, second, report.second_channel);
    let drawn: f64 = second.iter().sum();
    let left: f64 = faded.iter().sum();
    assert!(
        left <= drawn * FADED_LIGHT_LIMIT,
        "the faded card keeps {left} of {drawn} levels of light"
    );
    Ok(())
}

/// Builds a Bevy app with the real renderer, an invisible window and the production effect plugin.
///
/// libtest runs every test on a spawned thread, so winit must be allowed to run off the main
/// thread here, as in `client::build_graphical_app_off_main_thread`.
fn child_app(directory: PathBuf) -> TestResult<App> {
    let root = ResolvedAssetsRoot::resolve()?;
    let definition = BattleDefinition::load(root.as_path(), DEFAULT_BATTLE_SCENARIO.as_ref())?;
    let (styles, channels) = two_owners(&definition.palette);
    let mut window_backend = bevy::winit::WinitPlugin::<bevy::winit::WakeUp>::default();
    window_backend.run_on_any_thread = true;
    let mut app = App::new();
    app.add_plugins(
        client::presentation::GamePresentationPlugins
            .set(AssetPlugin {
                file_path: root.bevy_file_path()?.to_owned(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: (640.0_f32, 360.0_f32).into(),
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .set(window_backend),
    )
    .add_plugins((RenderCompatibilityPlugin, CombatFxPlugin))
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(Stage {
        directory,
        styles,
        channels,
        quad: None,
        shot: Shot::First,
        effect: None,
        slots: [None; 2],
        settled_frames: 0,
        first_attempts: 0,
        capture_pending: false,
        captured: None,
    })
    .add_systems(Startup, setup_stage)
    .add_systems(Update, run_shots);
    Ok(app)
}

/// The child of [`a_reused_slot_draws_its_new_effect`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn combat_fx_slot_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent test names the directory");
    let mut app = child_app(directory.into()).expect("the shipped assets must load");
    assert!(run_render_app(&mut app).is_success());
}
