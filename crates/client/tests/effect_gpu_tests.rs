//! The effect renderer's blocks on a real GPU (plan `2026-11-09-effect-visualization` WP4,
//! §10.2, D-9).
//!
//! # Responsibility
//! One parent test re-executes this binary through `support/gpu_run.rs` (radv with the shared
//! lock, or lavapipe; charter C21). The child runs Bevy's real renderer with the production
//! `client::effects::EffectsPlugin`, the shipped effect shaders and a test-only vocabulary over
//! the fixture texture set written into a temporary directory (charter §1.2: never under
//! `assets/`). It renders one capture per shot; the parent measures them:
//! - **soft depth**: a camera-facing card crossing the ground fades into it: no step above
//!   [`MAX_SOFT_STEP`] levels between vertically adjacent pixels along the intersection, where
//!   the same card with no soft depth shows a hard step (the control);
//! - **decal footprint**: every lit pixel of a projected disc decal lies on the ground inside
//!   its box's footprint plus 2 px, the disc covers most of its area, and no pixel of a
//!   vertical wall inside the box is lit (grazing fade);
//! - **ring radius**: a ground ring's outer and inner edges are within 2 px of their bound
//!   radii;
//! - **premultiplied mix**: an additive card (opacity 0) keeps the background under it, an
//!   alpha card (opacity 1) covers it, in one pass;
//! - **bubble ordering**: a card behind a blended sphere (the bubbles' phase and depth
//!   behaviour) is hidden by it while a card in front of it in the same frame is drawn over it;
//!   the control without the sphere shows the hidden card.
//! - **ribbon** (WP7): a camera-facing and a ground ribbon along lines of known ends: each
//!   strip's lit ends lie within 2 px of its projected line ends and its width within 2 px of
//!   twice its bound half width; a ribbon whose line is seen only from its third point on draws
//!   nothing before the cell edge half a spacing before that point.
//! - **distortion** (WP7): a disc over a striped ground bends the stripes inside it (many
//!   pixels change against the control without the disc) and changes no pixel more than 2 px
//!   outside its projected radius.
//!
//! # Invariants verified
//! As listed; each shot's statistics are printed for the driver comparison.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

#[path = "support/effect_fixtures.rs"]
mod effect_fixtures;

use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::camera::CameraProjection;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::effects::binding::BoundValues;
use client::effects::catalog::schema::Binding;
use client::effects::catalog::EffectCatalog;
use client::effects::placement::WorldLine;
use client::effects::runtime::{EffectRuntime, SpawnRequest};
use client::effects::systems::EffectCatalogSource;
use client::effects::EffectsPlugin;
use client::render_compatibility::{run_render_app, RenderCompatibilityPlugin};
use client::ResolvedAssetsRoot;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_EFFECT_GPU_CHILD_OUT";
/// The child test.
const CHILD_TEST: &str = "effect_gpu_child";
/// Capture size.
const WIDTH: u32 = 640;
/// Capture size.
const HEIGHT: u32 = 360;
/// Frames between setting a shot up and capturing it.
const SETTLE_FRAMES: u32 = 6;
/// Captures of the first shot allowed while the pipelines compile.
const MAX_FIRST_ATTEMPTS: u32 = 200;
/// Largest step in the red channel between vertically adjacent pixels of a soft card.
const MAX_SOFT_STEP: i32 = 24;
/// Pixel tolerance of footprints and radii.
const TOLERANCE_PX: f32 = 2.0;
/// Radius of the decal, world units.
const DECAL_RADIUS: f32 = 1.5;
/// Radius of the ring, world units.
const RING_RADIUS: f32 = 2.0;
/// Ring thickness as a fraction of the radius.
const RING_THICKNESS: f32 = 0.2;
/// Half width of the ribbons, world units.
const RIBBON_HALF_WIDTH: f32 = 0.25;
/// The ribbons' lines: x from, x to, z (camera-facing, ground, partly seen).
const RIBBON_LINES: [(f32, f32, f32); 3] = [(-2.0, 2.0, -1.6), (-2.5, 1.5, 0.0), (-2.0, 2.0, 1.6)];
/// Search spacing of the ribbons' lines, world units.
const RIBBON_SPACING: f32 = 0.5;
/// The first search point the viewer sees on the partly seen ribbon's line.
const RIBBON_FIRST_SEEN: u32 = 3;
/// World units over which a card clipped at the fog fades in (`CLIP_EDGE` of `sprite.wgsl`).
const RIBBON_CLIP_EDGE: f32 = 0.1;
/// Radius of the distortion disc, world units.
const WARP_RADIUS: f32 = 1.5;
/// Fewest pixels the distortion must change inside its disc.
const WARP_MIN_CHANGED: usize = 300;
/// Levels by which a pixel outside the disc may differ from the control.
const WARP_OUTSIDE_LEVELS: i32 = 2;

/// The test vocabulary's effects.
const EFFECTS: &str = r#"
    "soft_card": (class: Major, event: Point, layers: [(block: Sprite(texture: "solid",
        ramp: "red_add", soft_depth: 1.5), size: (scale: 1.0), duration: 100.0, priority: Critical)]),
    "hard_card": (class: Major, event: Point, layers: [(block: Sprite(texture: "solid",
        ramp: "red_add"), size: (scale: 1.0), duration: 100.0, priority: Critical)]),
    "stain": (class: Major, event: Point, layers: [(block: Decal(texture: "soft_disc",
        ramp: "blue", depth: 1.0, grazing: 0.35), size: (scale: 1.5), duration: 100.0,
        priority: Critical)]),
    "halo": (class: Major, event: Point, layers: [(block: Ring(texture: "solid", ramp: "green",
        orient: Ground, thickness: 0.2), radius: (from: (scale: 2.0), to: (scale: 2.0)),
        duration: 100.0, priority: Critical)]),
    "cover": (class: Major, event: Point, layers: [(block: Sprite(texture: "solid",
        ramp: "white"), size: (scale: 0.8), duration: 100.0, priority: Critical)]),
    "glow": (class: Major, event: Point, layers: [(block: Sprite(texture: "solid",
        ramp: "red_add"), size: (scale: 0.8), duration: 100.0, priority: Critical)]),
    "behind": (class: Major, event: Point, layers: [(block: Sprite(texture: "solid",
        ramp: "red_add"), size: (scale: 0.5), duration: 100.0, priority: Critical)]),
    "front": (class: Major, event: Point, layers: [(block: Sprite(texture: "solid",
        ramp: "green"), size: (scale: 0.4), duration: 100.0, priority: Critical)]),
    "strip": (class: Major, event: Line, layers: [(block: Ribbon(texture: "solid", ramp: "green",
        taper: (0.0, 0.0)), size: (scale: 0.25), duration: 100.0, priority: Critical)]),
    "ground_strip": (class: Major, event: Line, layers: [(block: Ribbon(texture: "solid",
        ramp: "green", orient: Ground, taper: (0.0, 0.0)), size: (scale: 0.25), duration: 100.0,
        priority: Critical)]),
    "warp": (class: Major, event: Point, layers: [(block: Distortion(noise: "grain",
        strength: 0.25), size: (scale: 1.5), duration: 100.0, priority: Critical)])
"#;

/// One capture: its camera, scene and effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shot {
    Soft,
    Hard,
    Decal,
    WallOnly,
    Ring,
    Mix,
    Bubble,
    BubbleHidden,
    Ribbon,
    Warp,
    WarpControl,
}

const SHOTS: [Shot; 11] = [
    Shot::Soft,
    Shot::Hard,
    Shot::Decal,
    Shot::WallOnly,
    Shot::Ring,
    Shot::Mix,
    Shot::Bubble,
    Shot::BubbleHidden,
    Shot::Ribbon,
    Shot::Warp,
    Shot::WarpControl,
];

impl Shot {
    fn name(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Hard => "hard",
            Self::Decal => "decal",
            Self::WallOnly => "wall_only",
            Self::Ring => "ring",
            Self::Mix => "mix",
            Self::Bubble => "bubble",
            Self::BubbleHidden => "bubble_hidden",
            Self::Ribbon => "ribbon",
            Self::Warp => "warp",
            Self::WarpControl => "warp_control",
        }
    }

    /// The camera transform of the shot.
    fn camera(self) -> Transform {
        match self {
            Self::Soft | Self::Hard => {
                Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y)
            }
            Self::Decal | Self::WallOnly => {
                Transform::from_xyz(0.0, 6.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)
            }
            Self::Ring | Self::Ribbon | Self::Warp | Self::WarpControl => {
                Transform::from_xyz(0.0, 10.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z)
            }
            Self::Mix | Self::Bubble | Self::BubbleHidden => {
                Transform::from_xyz(0.0, 0.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y)
            }
        }
    }

    /// Effects of the shot: name and anchor.
    fn effects(self) -> &'static [(&'static str, [f32; 3])] {
        match self {
            Self::Soft => &[("soft_card", [0.0, 0.5, 0.0])],
            Self::Hard => &[("hard_card", [0.0, 0.5, 0.0])],
            Self::Decal => &[("stain", [0.0, 0.0, 0.0])],
            Self::WallOnly => &[],
            Self::Ring => &[("halo", [0.0, 0.0, 0.0])],
            Self::Mix => &[("cover", [-0.5, 0.0, 0.0]), ("glow", [0.5, 0.0, 0.5])],
            Self::Bubble | Self::BubbleHidden => {
                &[("behind", [0.0, 0.0, -2.0]), ("front", [0.9, 0.0, 2.0])]
            }
            Self::Ribbon => &[
                ("strip", [0.0; 3]),
                ("ground_strip", [0.0; 3]),
                ("strip", [0.0; 3]),
            ],
            Self::Warp => &[("warp", [0.0; 3])],
            Self::WarpControl => &[],
        }
    }
}

/// Camera projection of every shot.
fn projection() -> PerspectiveProjection {
    PerspectiveProjection {
        aspect_ratio: WIDTH as f32 / HEIGHT as f32,
        ..default()
    }
}

/// Screen position (pixels, origin top left) of world point `point` seen from `camera`.
fn project(camera: &Transform, point: Vec3) -> Vec2 {
    let clip = projection().get_clip_from_view() * camera.compute_matrix().inverse();
    let ndc = clip.project_point3(point);
    Vec2::new(
        (ndc.x * 0.5 + 0.5) * WIDTH as f32,
        (0.5 - ndc.y * 0.5) * HEIGHT as f32,
    )
}

/// Where the ray through pixel centre `pixel` meets the ground plane y = 0.
fn ground_hit(camera: &Transform, pixel: Vec2) -> Option<Vec3> {
    let world_from_clip =
        (projection().get_clip_from_view() * camera.compute_matrix().inverse()).inverse();
    let ndc = Vec2::new(
        pixel.x / WIDTH as f32 * 2.0 - 1.0,
        1.0 - pixel.y / HEIGHT as f32 * 2.0,
    );
    let near = world_from_clip.project_point3(ndc.extend(1.0));
    let far = world_from_clip.project_point3(ndc.extend(0.001));
    let direction = far - near;
    if direction.y.abs() < 1e-6 {
        return None;
    }
    let t = -near.y / direction.y;
    (t > 0.0).then(|| near + direction * t)
}

// --- Child ---------------------------------------------------------------------------------

/// Marks the entities of the current shot.
#[derive(Component)]
struct ShotEntity;

/// The child's progress.
#[derive(Resource)]
struct Stage {
    directory: PathBuf,
    shot: usize,
    settled: u32,
    first_attempts: u32,
    capture_pending: bool,
    captured: Option<bool>,
    set_up: bool,
}

/// Spawns the scene of `shot`.
fn spawn_scene(
    shot: Shot,
    commands: &mut Commands<'_, '_>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let unlit = |materials: &mut Assets<StandardMaterial>, color: Color, alpha_mode: AlphaMode| {
        materials.add(StandardMaterial {
            base_color: color,
            unlit: true,
            alpha_mode,
            ..default()
        })
    };
    let ground = matches!(shot, Shot::Soft | Shot::Hard | Shot::Decal | Shot::WallOnly);
    if ground {
        let material = unlit(
            materials,
            Color::linear_rgb(0.1, 0.1, 0.1),
            AlphaMode::Opaque,
        );
        commands.spawn((
            ShotEntity,
            Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
            MeshMaterial3d(material),
        ));
    }
    if matches!(shot, Shot::Decal | Shot::WallOnly) {
        let material = unlit(
            materials,
            Color::linear_rgb(0.0, 0.6, 0.0),
            AlphaMode::Opaque,
        );
        commands.spawn((
            ShotEntity,
            Mesh3d(meshes.add(Cuboid::new(0.3, 1.5, 2.6))),
            MeshMaterial3d(material),
            Transform::from_xyz(0.75, 0.75, 0.0),
        ));
    }
    if shot == Shot::Mix {
        let material = unlit(
            materials,
            Color::linear_rgb(0.0, 0.0, 0.5),
            AlphaMode::Opaque,
        );
        commands.spawn((
            ShotEntity,
            Mesh3d(meshes.add(Rectangle::new(40.0, 40.0))),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, 0.0, -3.0),
        ));
    }
    if matches!(shot, Shot::Warp | Shot::WarpControl) {
        // Alternating stripes across x, so a bend shows as a moved edge.
        let light = unlit(
            materials,
            Color::linear_rgb(0.8, 0.8, 0.8),
            AlphaMode::Opaque,
        );
        let dark = unlit(
            materials,
            Color::linear_rgb(0.05, 0.05, 0.3),
            AlphaMode::Opaque,
        );
        let stripe = meshes.add(Cuboid::new(0.25, 0.02, 8.0));
        for index in -16_i32..16 {
            let material = if index % 2 == 0 { &light } else { &dark };
            commands.spawn((
                ShotEntity,
                Mesh3d(stripe.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(index as f32 * 0.25 + 0.125, -0.01, 0.0),
            ));
        }
    }
    if shot == Shot::Bubble {
        let material = unlit(
            materials,
            Color::linear_rgba(0.0, 0.0, 1.0, 1.0),
            AlphaMode::Blend,
        );
        commands.spawn((
            ShotEntity,
            Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(64, 32))),
            MeshMaterial3d(material),
        ));
    }
}

/// The `index`th ribbon line of [`RIBBON_LINES`], with the viewer's seen mask.
fn ribbon_line(index: usize) -> WorldLine {
    let (from, to, z) = RIBBON_LINES[index];
    let mut line = WorldLine::whole(Vec3::new(from, 0.0, z), Vec3::new(to, 0.0, z));
    if index == 2 {
        line.seen = u64::MAX << RIBBON_FIRST_SEEN;
    }
    line
}

/// Spawns the effects of `shot`.
fn spawn_effects(shot: Shot, catalog: &EffectCatalog, runtime: &mut EffectRuntime, now: f64) {
    runtime.clear();
    for (index, (name, anchor)) in shot.effects().iter().enumerate() {
        let effect = catalog
            .effect_id(name)
            .expect("the vocabulary defines every shot's effect");
        let (anchor, bound, line) = if shot == Shot::Ribbon {
            let line = ribbon_line(index);
            let bound = BoundValues::default()
                .with(Binding::LineLength, line.from.distance(line.to))
                .with(Binding::LineSpacing, RIBBON_SPACING);
            (line.from, bound, Some(line))
        } else {
            (Vec3::from_array(*anchor), BoundValues::default(), None)
        };
        let request = SpawnRequest {
            effect,
            anchor,
            bound,
            line,
            delay: 0.0,
        };
        runtime
            .spawn(catalog, &request, now)
            .expect("the pool has room");
    }
}

/// Advances the shots: sets one up, waits, captures it, and exits after the last.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn run_shots(
    mut commands: Commands<'_, '_>,
    mut stage: ResMut<'_, Stage>,
    catalog: Option<Res<'_, EffectCatalog>>,
    runtime: Option<ResMut<'_, EffectRuntime>>,
    time: Res<'_, Time<Virtual>>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    shot_entities: Query<'_, '_, Entity, With<ShotEntity>>,
    mut cameras: Query<'_, '_, &mut Transform, With<Camera3d>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let (Some(catalog), Some(mut runtime)) = (catalog, runtime) else {
        return;
    };
    if stage.capture_pending {
        return;
    }
    if let Some(lit) = stage.captured.take() {
        stage.settled = 0;
        if stage.shot == 0 && !lit {
            stage.first_attempts += 1;
            assert!(
                stage.first_attempts < MAX_FIRST_ATTEMPTS,
                "the first shot never drew"
            );
            return;
        }
        stage.shot += 1;
        stage.set_up = false;
    }
    let Some(&shot) = SHOTS.get(stage.shot) else {
        exit.send(AppExit::Success);
        return;
    };
    if !stage.set_up {
        for entity in &shot_entities {
            commands.entity(entity).despawn_recursive();
        }
        spawn_scene(shot, &mut commands, &mut meshes, &mut materials);
        spawn_effects(shot, &catalog, &mut runtime, time.elapsed_secs_f64());
        for mut transform in &mut cameras {
            *transform = shot.camera();
        }
        stage.set_up = true;
    }
    stage.settled += 1;
    if stage.settled < SETTLE_FRAMES {
        return;
    }
    stage.capture_pending = true;
    let path = stage.directory.join(format!("{}.png", shot.name()));
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
            // The first shot's card adds pure red over grey: lit once the pipeline exists.
            stage.captured = Some(
                image
                    .pixels()
                    .any(|pixel| pixel.0[0] > 200 && pixel.0[1] < 100),
            );
            stage.capture_pending = false;
        },
    );
}

/// Spawns the camera.
fn setup_camera(mut commands: Commands<'_, '_>) {
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(projection()),
        Tonemapping::None,
        DepthPrepass,
        Shot::Soft.camera(),
    ));
}

/// The child app: the real renderer, an invisible window and the production effect plugin.
fn child_app(directory: PathBuf) -> TestResult<App> {
    let root = ResolvedAssetsRoot::resolve()?;
    let manifest = effect_fixtures::write_fixture_set(&directory);
    let noise = effect_fixtures::write_noise_set(&directory);
    let vocabulary = directory.join("effects.ron");
    std::fs::write(
        &vocabulary,
        effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", EFFECTS),
    )?;
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
                    resolution: (WIDTH as f32, HEIGHT as f32).into(),
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .set(window_backend),
    )
    .add_plugins((RenderCompatibilityPlugin, EffectsPlugin))
    .add_event::<client::CombatVisualEvent>()
    .insert_resource(root)
    .insert_resource(EffectCatalogSource(vocabulary))
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(Stage {
        directory,
        shot: 0,
        settled: 0,
        first_attempts: 0,
        capture_pending: false,
        captured: None,
        set_up: false,
    })
    .add_systems(Startup, setup_camera)
    .add_systems(Update, run_shots);
    Ok(app)
}

/// The child of [`effect_blocks_render_on_the_gpu`].
#[test]
#[ignore = "launched by effect_blocks_render_on_the_gpu on a virtual display"]
fn effect_gpu_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names the directory");
    let mut app = child_app(directory.into()).expect("the child app builds");
    assert!(run_render_app(&mut app).is_success());
}

// --- Parent --------------------------------------------------------------------------------

/// A capture.
struct Capture(image::RgbImage);

impl Capture {
    fn load(directory: &Path, shot: Shot) -> TestResult<Self> {
        Ok(Self(
            image::open(directory.join(format!("{}.png", shot.name())))?.to_rgb8(),
        ))
    }

    fn at(&self, x: u32, y: u32) -> [i32; 3] {
        let pixel = self.0.get_pixel(x, y).0;
        [
            i32::from(pixel[0]),
            i32::from(pixel[1]),
            i32::from(pixel[2]),
        ]
    }

    fn pixels(&self) -> impl Iterator<Item = (u32, u32, [i32; 3])> + '_ {
        self.0
            .enumerate_pixels()
            .map(|(x, y, pixel)| (x, y, pixel.0.map(i32::from)))
    }
}

/// Largest red step between vertically adjacent pixels in the columns the card covers, from
/// below its top edge to the bottom of the image, and the number of columns measured.
fn largest_vertical_step(capture: &Capture) -> (i32, u32) {
    let camera = Shot::Soft.camera();
    let left = project(&camera, Vec3::new(-0.8, 0.5, 0.0)).x as u32;
    let right = project(&camera, Vec3::new(0.8, 0.5, 0.0)).x as u32;
    // Below the card's top edge (at 1.5), which is a real edge of the card, down to the ground.
    let top = project(&camera, Vec3::new(0.0, 1.2, 0.0)).y as u32;
    let mut largest = 0;
    let mut columns = 0;
    for x in left..=right {
        let mut column_max = 0;
        for y in top.max(1)..HEIGHT {
            let step = (capture.at(x, y)[0] - capture.at(x, y - 1)[0]).abs();
            column_max = column_max.max(step);
        }
        columns += 1;
        largest = largest.max(column_max);
    }
    (largest, columns)
}

/// Checks the soft card against its hard control.
fn check_soft_depth(directory: &Path) -> TestResult<String> {
    let (soft, soft_columns) = largest_vertical_step(&Capture::load(directory, Shot::Soft)?);
    let (hard, _) = largest_vertical_step(&Capture::load(directory, Shot::Hard)?);
    assert!(
        soft_columns > 50,
        "the card spans too few columns: {soft_columns}"
    );
    assert!(
        hard > MAX_SOFT_STEP,
        "control: the hard card must show its intersection ({hard})"
    );
    assert!(
        soft <= MAX_SOFT_STEP,
        "the soft card steps {soft} levels where it meets the ground (limit {MAX_SOFT_STEP})"
    );
    Ok(format!(
        "soft depth: largest step soft {soft}, hard {hard} over {soft_columns} columns"
    ))
}

/// Whether pixel `(x, y)` of the wall mask has every 8-neighbour in the mask too.
fn interior(mask: &[bool], x: u32, y: u32) -> bool {
    if x == 0 || y == 0 || x + 1 >= WIDTH || y + 1 >= HEIGHT {
        return false;
    }
    (y - 1..=y + 1).all(|ny| (x - 1..=x + 1).all(|nx| mask[(ny * WIDTH + nx) as usize]))
}

/// Checks the decal's footprint and its absence on the wall.
fn check_decal(directory: &Path) -> TestResult<String> {
    let decal = Capture::load(directory, Shot::Decal)?;
    let control = Capture::load(directory, Shot::WallOnly)?;
    let camera = Shot::Decal.camera();
    let wall: Vec<bool> = control
        .pixels()
        .map(|(_, _, pixel)| pixel[1] > 100 && pixel[0] < 40)
        .collect();
    let mut lit = 0_u32;
    let mut outside = 0_u32;
    let mut on_wall = 0_u32;
    for (x, y, pixel) in decal.pixels() {
        let base = control.at(x, y);
        if pixel[2] - base[2] <= 24 {
            continue;
        }
        if wall[(y * WIDTH + x) as usize] {
            if interior(&wall, x, y) {
                on_wall += 1;
            }
            continue;
        }
        lit += 1;
        let centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
        let Some(hit) = ground_hit(&camera, centre) else {
            outside += 1;
            continue;
        };
        let radial = Vec2::new(hit.x, hit.z);
        if radial.length() > DECAL_RADIUS {
            let edge = radial.normalize() * DECAL_RADIUS;
            let edge_px = project(&camera, Vec3::new(edge.x, 0.0, edge.y));
            if edge_px.distance(centre) > TOLERANCE_PX + 0.71 {
                outside += 1;
            }
        }
    }
    // The disc covers 0.9 of the box radius; part of it is hidden behind the wall.
    let rim: Vec<Vec2> = (0..64)
        .map(|i| {
            let angle = i as f32 / 64.0 * std::f32::consts::TAU;
            project(
                &camera,
                Vec3::new(angle.cos(), 0.0, angle.sin()) * DECAL_RADIUS * 0.9,
            )
        })
        .collect();
    let area = polygon_area(&rim);
    assert!(
        lit as f32 > area * 0.4,
        "the decal lit {lit} px of a {area:.0} px disc"
    );
    assert_eq!(
        outside, 0,
        "{outside} decal pixels outside the box footprint + {TOLERANCE_PX} px"
    );
    assert_eq!(on_wall, 0, "{on_wall} decal pixels on the vertical wall");
    Ok(format!(
        "decal: {lit} px lit of a {area:.0} px disc, {outside} outside the footprint, {on_wall} on the wall"
    ))
}

/// Area of a simple polygon, square pixels.
fn polygon_area(points: &[Vec2]) -> f32 {
    let mut twice = 0.0;
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        twice += point.x * next.y - next.x * point.y;
    }
    twice.abs() * 0.5
}

/// Checks the ring's outer and inner radius.
fn check_ring(directory: &Path) -> TestResult<String> {
    let capture = Capture::load(directory, Shot::Ring)?;
    let camera = Shot::Ring.camera();
    let centre = project(&camera, Vec3::ZERO);
    let outer_px = project(&camera, Vec3::new(RING_RADIUS, 0.0, 0.0)).distance(centre);
    let inner_px = outer_px * (1.0 - RING_THICKNESS);
    let (mut outer, mut inner, mut count) = (0.0_f32, f32::MAX, 0_u32);
    for (x, y, pixel) in capture.pixels() {
        if pixel[1] < 128 {
            continue;
        }
        count += 1;
        let distance = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(centre);
        outer = outer.max(distance);
        inner = inner.min(distance);
    }
    assert!(count > 500, "the ring lit only {count} px");
    assert!(
        (outer - outer_px).abs() <= TOLERANCE_PX,
        "outer radius {outer:.2} px, bound {outer_px:.2} px"
    );
    assert!(
        (inner - inner_px).abs() <= TOLERANCE_PX,
        "inner radius {inner:.2} px, bound {inner_px:.2} px"
    );
    Ok(format!(
        "ring: outer {outer:.2} px (bound {outer_px:.2}), inner {inner:.2} px (bound {inner_px:.2}), {count} px"
    ))
}

/// Checks every ribbon's ends and width, and the partly seen ribbon's clip.
fn check_ribbon(directory: &Path) -> TestResult<String> {
    let capture = Capture::load(directory, Shot::Ribbon)?;
    let camera = Shot::Ribbon.camera();
    let mut report = Vec::with_capacity(RIBBON_LINES.len());
    for (index, (from, to, z)) in RIBBON_LINES.into_iter().enumerate() {
        // Lit pixels whose ground point lies within a unit of this line, across.
        let (mut low, mut high, mut near, mut far, mut count) =
            (f32::MAX, f32::MIN, f32::MAX, f32::MIN, 0_u32);
        for (x, y, pixel) in capture.pixels() {
            if pixel[1] < 128 {
                continue;
            }
            let Some(point) = ground_hit(&camera, Vec2::new(x as f32 + 0.5, y as f32 + 0.5)) else {
                continue;
            };
            if (point.z - z).abs() > 1.0 {
                continue;
            }
            count += 1;
            let pixel = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            low = low.min(pixel.x);
            high = high.max(pixel.x);
            near = near.min(pixel.y);
            far = far.max(pixel.y);
        }
        assert!(count > 200, "ribbon {index} lit only {count} px");
        let line = ribbon_line(index);
        let start_x = if index == 2 {
            // Clipped half a spacing before the first seen point.
            from + (RIBBON_FIRST_SEEN as f32 - 0.5) * RIBBON_SPACING
        } else {
            line.from.x
        };
        let start = project(&camera, Vec3::new(start_x, 0.0, z)).x;
        let end = project(&camera, Vec3::new(to, 0.0, z)).x;
        let width = project(&camera, Vec3::new(0.0, 0.0, z + RIBBON_HALF_WIDTH)).y
            - project(&camera, Vec3::new(0.0, 0.0, z - RIBBON_HALF_WIDTH)).y;
        // Pixel centres: the outermost lit centres lie up to half a pixel inside each edge.
        let errors = [low - start, high - end, (far - near + 1.0) - width.abs()];
        // Where the fog cuts the strip it fades in over the sprite shader's clip edge (0.1
        // world units) from the cell edge on, never before it.
        let fade_px = if index == 2 {
            project(&camera, Vec3::new(start_x + RIBBON_CLIP_EDGE, 0.0, z)).x - start
        } else {
            0.0
        };
        assert!(
            errors[0] >= -TOLERANCE_PX
                && errors[0] <= TOLERANCE_PX + fade_px
                && errors[1..].iter().all(|error| error.abs() <= TOLERANCE_PX),
            "ribbon {index}: ends {low:.1}..{high:.1} px (line {start:.1}..{end:.1}), width {:.1} px (bound {:.1}); errors {errors:?}",
            far - near + 1.0,
            width.abs()
        );
        report.push(format!(
            "ribbon {index}: start {:+.2} px, end {:+.2} px, width {:+.2} px",
            errors[0], errors[1], errors[2]
        ));
    }
    Ok(report.join("; "))
}

/// Checks that the distortion disc bends the stripes inside it and nothing outside it.
fn check_warp(directory: &Path) -> TestResult<String> {
    let warp = Capture::load(directory, Shot::Warp)?;
    let control = Capture::load(directory, Shot::WarpControl)?;
    let camera = Shot::Warp.camera();
    let centre = project(&camera, Vec3::ZERO);
    let radius_px = project(&camera, Vec3::new(WARP_RADIUS, 0.0, 0.0)).distance(centre);
    let (mut inside, mut outside, mut worst_outside) = (0_usize, 0_usize, 0_i32);
    for (x, y, pixel) in warp.pixels() {
        let other = control.at(x, y);
        let largest = (0..3)
            .map(|channel| (pixel[channel] - other[channel]).abs())
            .max()
            .unwrap_or(0);
        let distance = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(centre);
        if distance > radius_px + TOLERANCE_PX {
            worst_outside = worst_outside.max(largest);
            outside += usize::from(largest > WARP_OUTSIDE_LEVELS);
        } else if largest > 40 {
            inside += 1;
        }
    }
    assert!(
        inside >= WARP_MIN_CHANGED,
        "the disc bent only {inside} px of the stripes inside it"
    );
    assert_eq!(
        outside, 0,
        "{outside} px changed outside the disc (largest {worst_outside} levels)"
    );
    Ok(format!(
        "distortion: {inside} px bent inside the disc (radius {radius_px:.1} px), none outside (largest {worst_outside} levels)"
    ))
}

/// Checks that additive and alpha cards mix in one premultiplied pass.
fn check_mix(directory: &Path) -> TestResult<String> {
    let capture = Capture::load(directory, Shot::Mix)?;
    let camera = Shot::Mix.camera();
    let sample = |point: Vec3| {
        let pixel = project(&camera, point);
        capture.at(pixel.x as u32, pixel.y as u32)
    };
    let background = sample(Vec3::new(0.0, 2.5, -3.0));
    let additive = sample(Vec3::new(1.1, 0.0, 0.5));
    let alpha = sample(Vec3::new(-1.1, 0.0, 0.0));
    assert!(
        background[2] > 100 && background[0] < 10,
        "background {background:?}"
    );
    assert!(
        additive[0] >= 250,
        "the additive card adds red: {additive:?}"
    );
    assert!(
        (additive[2] - background[2]).abs() <= 8,
        "the additive card keeps the background under it: {additive:?} over {background:?}"
    );
    assert!(
        alpha.iter().all(|level| *level >= 250),
        "the alpha card covers: {alpha:?}"
    );
    Ok(format!(
        "mix: background {background:?}, additive {additive:?}, alpha {alpha:?}"
    ))
}

/// Checks the ordering of cards against a blended sphere.
fn check_bubble(directory: &Path) -> TestResult<String> {
    let with = Capture::load(directory, Shot::Bubble)?;
    let without = Capture::load(directory, Shot::BubbleHidden)?;
    let camera = Shot::Bubble.camera();
    let centre = project(&camera, Vec3::ZERO);
    let radius_px = project(&camera, Vec3::new(1.0, 0.0, 0.0)).distance(centre) * 0.95;
    let red = |pixel: [i32; 3]| pixel[0] > 40 && pixel[1] < 40;
    let green = |pixel: [i32; 3]| pixel[1] > 128 && pixel[0] < 40;
    let hidden_red = with.pixels().filter(|(_, _, pixel)| red(*pixel)).count();
    let control_red = without.pixels().filter(|(_, _, pixel)| red(*pixel)).count();
    let front_over = with
        .pixels()
        .filter(|(x, y, pixel)| {
            green(*pixel)
                && Vec2::new(*x as f32 + 0.5, *y as f32 + 0.5).distance(centre) < radius_px
        })
        .count();
    assert!(
        control_red > 100,
        "control: the card behind draws without the sphere ({control_red} px)"
    );
    assert_eq!(
        hidden_red, 0,
        "{hidden_red} px of the card behind the sphere drawn over it"
    );
    assert!(
        front_over > 100,
        "the card in front must draw over the sphere ({front_over} px)"
    );
    Ok(format!(
        "bubble: behind card {hidden_red} px over the sphere (control {control_red} px), front card {front_over} px over it"
    ))
}

/// WP4: every block draws as specified on the selected driver.
#[test]
fn effect_blocks_render_on_the_gpu() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    let driver = command.driver();
    command.assert_passes_validation("effect block child");
    if let Some(keep) = std::env::var_os("SPACE_SCUFFLE_EFFECT_GPU_KEEP") {
        let keep = PathBuf::from(keep);
        std::fs::create_dir_all(&keep)?;
        for shot in SHOTS {
            let name = format!("{}.png", shot.name());
            std::fs::copy(directory.path().join(&name), keep.join(&name))?;
        }
    }
    let report = [
        check_soft_depth(directory.path())?,
        check_decal(directory.path())?,
        check_ring(directory.path())?,
        check_mix(directory.path())?,
        check_bubble(directory.path())?,
        check_ribbon(directory.path())?,
        check_warp(directory.path())?,
    ];
    eprintln!(
        "effect GPU statistics on {driver:?}:\n  {}",
        report.join("\n  ")
    );
    Ok(())
}
