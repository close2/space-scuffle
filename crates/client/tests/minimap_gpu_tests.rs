//! The minimap on a real GPU: the real shader, the theme's real albedo arrays and the production
//! client (plan `2026-12-01-minimap` §6, G-1 and G-6).
//!
//! # Responsibility
//! Each child plays a skirmish on a real window under the GPU test harness, stops the clock once
//! the minimap has been drawn, and reads back the **minimap texture itself** with Bevy's
//! `gpu_readback::Readback`, so pixels are addressed exactly by the layout the child reports. The
//! parent asserts on those pixels:
//! - G-1, terrain: a flat cell of cliff level `k` is at least [`LEVEL_STEP`] brighter in linear
//!   luminance than a cell of level `k − 1` of the same ground; an unwalkable cell is within
//!   [`UNPATHABLE_TOLERANCE`] of `unpathable_color`; no pixel on the map is black; and, on a map
//!   taller than it is wide, the letterbox is the background.
//! - G-6, in the HUD: a window screenshot taken after the readback shows, inside
//!   `HudGeometry::minimap`, the texture's pixels within [`HUD_TOLERANCE`], so egui shows the image
//!   the node wrote.
//!
//! # How it fits
//! The children re-execute this test binary through `support/gpu_run.rs` (charter §7), which
//! takes the GPU lock per child: radv locally, lavapipe on CI. The letterbox child runs on a
//! temporary assets root that links every shipped directory and adds one map, the shipped
//! skirmish map with 16 rows appended, so nothing shipped changes. The CPU claims are in
//! `minimap_tests.rs`.
//!
//! # Invariants verified
//! - The texture is read back after at least [`DRAWN_FRAMES`] frames of the node, with the clock
//!   stopped, so the image is complete and still.
//! - Setting `SPACE_SCUFFLE_MINIMAP_KEEP` to a directory makes the parent copy the pictures there,
//!   for review; it changes nothing the test asserts.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::battle_hud::HudGeometry;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::minimap::render::MinimapRenderEvidence;
use client::minimap::style::MinimapStyle;
use client::minimap::MinimapFrame;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::terrain::TerrainState;
use client::{ClientMode, LabState};
use serde::{Deserialize, Serialize};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable naming the directory the child writes to.
const CHILD_OUT_VAR: &str = "SPACE_SCUFFLE_MINIMAP_CHILD_OUT";
/// Environment variable naming the scenario the child plays, beneath its assets root.
const CHILD_SCENARIO_VAR: &str = "SPACE_SCUFFLE_MINIMAP_SCENARIO";
/// Environment variable naming a directory the parent copies the pictures to, for review.
const KEEP_VAR: &str = "SPACE_SCUFFLE_MINIMAP_KEEP";
/// Name of the libtest entry every parent launches.
const CHILD_TEST: &str = "minimap_child";
/// Frame cap that makes the media policy strict. The GPU run's own deadline ends a stuck child.
const CHILD_FRAME_LIMIT: u64 = 100_000;
/// File the child writes its [`MinimapShot`] to.
const REPORT_FILE: &str = "minimap.ron";
/// Frames the node must have drawn, with the clock stopped, before the readback.
const DRAWN_FRAMES: u64 = 8;
/// Rows appended to the skirmish map for the letterbox child.
const EXTRA_ROWS: u32 = 16;

/// Smallest ratio of the linear luminance of level `k` to level `k − 1` on the same ground: the
/// shipped brightness steps are 1.27, 1.21 and 1.18.
const LEVEL_STEP: f64 = 1.10;
/// Largest difference per sRGB channel, in `[0, 1]`, between an unwalkable cell and
/// `unpathable_color`.
const UNPATHABLE_TOLERANCE: f64 = 0.05;
/// Largest difference per channel, in levels, between the HUD and the texture.
const HUD_TOLERANCE: u8 = 6;

/// What a child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct MinimapShot {
    /// Side of the minimap texture in pixels.
    side: u32,
    /// Pixel of the map's corner.
    offset: [f32; 2],
    /// Pixels per world unit.
    scale: f32,
    /// Map width and height in cells.
    map: [u32; 2],
    /// The cell texels (`minimap/terrain.rs`), row-major RGBA8.
    cells: Vec<u8>,
    /// The terrain kind of each cell, as `TerrainKind as u8`, row-major.
    kinds: Vec<u8>,
    /// `unpathable_color`, sRGB.
    unpathable: [f32; 3],
    /// `background`, sRGB.
    background: [f32; 3],
    /// `HudGeometry::minimap` in egui points: `[left, top, right, bottom]`.
    hud: [f32; 4],
    /// Frames the node drew before the readback.
    drawn: u64,
}

/// A readback, unpadded: `side × side` RGBA8 pixels, sRGB-encoded.
struct Texture {
    side: u32,
    pixels: Vec<u8>,
}

impl Texture {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * self.side + x) * 4) as usize;
        self.pixels[index..index + 4]
            .try_into()
            .expect("four bytes")
    }
}

/// Runs the child on `scenario`, under `assets` if given, and returns its report, texture and
/// directory.
fn run_child(
    scenario: &str,
    assets: Option<&Path>,
    what: &str,
) -> TestResult<(MinimapShot, Texture, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.env(CHILD_SCENARIO_VAR, scenario);
    if let Some(assets) = assets {
        command.env("SPACE_SCUFFLE_ASSETS", assets);
    }
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation(what);
    let shot: MinimapShot = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    let texture = image::open(directory.path().join("texture.png"))?.to_rgba8();
    assert_eq!((texture.width(), texture.height()), (shot.side, shot.side));
    let texture = Texture {
        side: shot.side,
        pixels: texture.into_raw(),
    };
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        let keep = Path::new(&keep).join(what.replace(' ', "_"));
        std::fs::create_dir_all(&keep)?;
        for file in ["texture.png", "window.png", REPORT_FILE] {
            if directory.path().join(file).exists() {
                std::fs::copy(directory.path().join(file), keep.join(file))?;
            }
        }
    }
    Ok((shot, texture, directory))
}

/// sRGB byte to linear.
fn linear(byte: u8) -> f64 {
    let value = f64::from(byte) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear luminance of an sRGB pixel (Rec. 709 weights).
fn luminance(pixel: [u8; 4]) -> f64 {
    0.2126 * linear(pixel[0]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[2])
}

/// The texture pixel at the centre of cell `(x, y)`.
fn cell_pixel(shot: &MinimapShot, texture: &Texture, x: u32, y: u32) -> [u8; 4] {
    let px = shot.offset[0] + (x as f32 + 0.5) * shot.scale;
    let py = shot.offset[1] + (y as f32 + 0.5) * shot.scale;
    texture.at(px as u32, py as u32)
}

/// The cell texel of `(x, y)` and its kind, or `None` off the map.
fn cell(shot: &MinimapShot, x: i64, y: i64) -> Option<([u8; 4], u8)> {
    let [w, h] = shot.map;
    if x < 0 || y < 0 || x >= i64::from(w) || y >= i64::from(h) {
        return None;
    }
    let index = (y as usize) * w as usize + x as usize;
    let texel = shot.cells[index * 4..index * 4 + 4]
        .try_into()
        .expect("four bytes");
    Some((texel, shot.kinds[index]))
}

/// Whether cell `(x, y)` and its 3×3 neighbourhood share its texel and kind: a cell whose
/// centre pixel shows only itself, whatever filtering is near.
fn uniform(shot: &MinimapShot, x: u32, y: u32) -> bool {
    let Some(centre) = cell(shot, i64::from(x), i64::from(y)) else {
        return false;
    };
    (-1..=1).all(|dy| {
        (-1..=1).all(|dx| cell(shot, i64::from(x) + dx, i64::from(y) + dy) == Some(centre))
    })
}

/// Every cell of the map, as `(x, y)`.
fn every_cell(shot: &MinimapShot) -> impl Iterator<Item = (u32, u32)> {
    let [w, h] = shot.map;
    (0..h).flat_map(move |y| (0..w).map(move |x| (x, y)))
}

/// G-1's per-cell claims: levels brighten, unwalkable ground has its colour, nothing is black.
fn assert_terrain(shot: &MinimapShot, texture: &Texture) {
    // The brightest uniform flat walkable cell of each (level, kind), by its red byte.
    let mut by_level: std::collections::BTreeMap<(u8, u8), f64> = std::collections::BTreeMap::new();
    let mut unpathable = 0;
    for (x, y) in every_cell(shot) {
        let Some((texel, kind)) = cell(shot, i64::from(x), i64::from(y)) else {
            continue;
        };
        if !uniform(shot, x, y) {
            continue;
        }
        let pixel = cell_pixel(shot, texture, x, y);
        if texel[1] == 0 {
            for (channel, expected) in pixel.iter().zip(shot.unpathable) {
                let got = f64::from(*channel) / 255.0;
                assert!(
                    (got - f64::from(expected)).abs() <= UNPATHABLE_TOLERANCE,
                    "unwalkable cell ({x}, {y}) is {pixel:?}, not {:?}",
                    shot.unpathable
                );
            }
            unpathable += 1;
        } else if texel[2] == 0 && texel[0] % 85 == 0 {
            by_level.insert((texel[0] / 85, kind), luminance(pixel));
        }
    }
    assert!(unpathable > 0, "the map has unwalkable cells to check");
    let mut steps = 0;
    for (&(level, kind), &bright) in &by_level {
        let Some(&dim) = level
            .checked_sub(1)
            .and_then(|below| by_level.get(&(below, kind)))
        else {
            continue;
        };
        eprintln!(
            "level {level} over {} on kind {kind}: {bright:.4} / {dim:.4}",
            level - 1
        );
        assert!(
            bright >= LEVEL_STEP * dim,
            "level {level} ({bright:.4}) is not {LEVEL_STEP}x level {} ({dim:.4}) on kind {kind}",
            level - 1
        );
        steps += 1;
    }
    assert!(steps > 0, "some ground shows two levels: {by_level:?}");
    for y in 0..texture.side {
        for x in 0..texture.side {
            let world = Vec2::new(
                (x as f32 + 0.5 - shot.offset[0]) / shot.scale,
                (y as f32 + 0.5 - shot.offset[1]) / shot.scale,
            );
            let on_map = world.x >= 0.0
                && world.y >= 0.0
                && world.x < shot.map[0] as f32
                && world.y < shot.map[1] as f32;
            if on_map {
                let pixel = texture.at(x, y);
                assert_ne!(
                    &pixel[..3],
                    &[0, 0, 0],
                    "pixel ({x}, {y}) on the map is black"
                );
            }
        }
    }
}

/// G-1: terrain on the shipped skirmish, levels, unwalkable ground and no black pixel.
#[test]
fn g1_the_terrain_shows_levels_and_unwalkable_ground_on_the_real_shader() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let (shot, texture, _directory) = run_child(SKIRMISH_SCENARIO, None, "minimap terrain")?;
    eprintln!(
        "driver {}: side {} offset {:?} scale {} after {} frames",
        gpu_run::selected_driver().driver,
        shot.side,
        shot.offset,
        shot.scale,
        shot.drawn
    );
    assert!(shot.drawn >= DRAWN_FRAMES);
    assert_terrain(&shot, &texture);
    Ok(())
}

/// G-1: on a map taller than wide, the bands left and right of it are the background.
#[test]
fn g1_the_letterbox_of_a_tall_map_is_the_background() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    let scenario = tall_root(root.path())?;
    let (shot, texture, _directory) = run_child(&scenario, Some(root.path()), "minimap letterbox")?;
    assert_eq!(shot.map, [64, 64 + EXTRA_ROWS]);
    assert!(
        shot.offset[0] > 4.0,
        "letterboxed left and right: {:?}",
        shot.offset
    );
    let background: Vec<u8> = shot
        .background
        .iter()
        .map(|channel| (channel * 255.0).round() as u8)
        .collect();
    let band = (shot.offset[0] - 1.0).floor() as u32;
    for y in 0..texture.side {
        for x in (0..band).chain(texture.side - band..texture.side) {
            assert_eq!(
                &texture.at(x, y)[..3],
                background.as_slice(),
                "letterbox pixel ({x}, {y})"
            );
        }
    }
    assert_terrain(&shot, &texture);
    Ok(())
}

/// G-6: the window shows, inside `HudGeometry::minimap`, the texture the node wrote.
#[test]
fn g6_the_hud_shows_the_texture_the_node_wrote() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let (shot, texture, directory) = run_child(SKIRMISH_SCENARIO, None, "minimap hud")?;
    let window = image::open(directory.path().join("window.png"))?.to_rgba8();
    let [left, top, right, bottom] = shot.hud;
    // The virtual display has one pixel per point, so the square is the texture at 1:1.
    assert!(
        ((right - left) - shot.side as f32).abs() < 0.5
            && ((bottom - top) - shot.side as f32).abs() < 0.5,
        "the square {:?} is the texture's side {}",
        shot.hud,
        shot.side
    );
    let (mut compared, mut differing) = (0_u64, Vec::new());
    for y in 1..texture.side - 1 {
        for x in 1..texture.side - 1 {
            let centre = texture.at(x, y);
            let flat = [x - 1, x, x + 1].into_iter().all(|nx| {
                [y - 1, y, y + 1].into_iter().all(|ny| {
                    let other = texture.at(nx, ny);
                    (0..3).all(|c| other[c].abs_diff(centre[c]) <= 2)
                })
            });
            if !flat {
                continue;
            }
            let (wx, wy) = (
                (left + x as f32 + 0.5) as u32,
                (top + y as f32 + 0.5) as u32,
            );
            let shown = window.get_pixel(wx, wy).0;
            compared += 1;
            if (0..3).any(|c| shown[c].abs_diff(centre[c]) > HUD_TOLERANCE) {
                differing.push(((x, y), centre, shown));
            }
        }
    }
    eprintln!(
        "{compared} flat texture pixels compared, {} differ",
        differing.len()
    );
    assert!(
        compared > u64::from(shot.side) * 10,
        "enough flat pixels: {compared}"
    );
    assert!(
        differing.is_empty(),
        "{} of {compared} pixels differ, first {:?}",
        differing.len(),
        differing.first()
    );
    Ok(())
}

/// Builds the letterbox child's assets root in `root`: every shipped entry linked, plus
/// `maps/minimap_tall.scmap.ron` (the skirmish map with [`EXTRA_ROWS`] copies of its last row)
/// and a scenario playing it. Returns the scenario's path beneath the root.
fn tall_root(root: &Path) -> TestResult<String> {
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()?;
    for entry in std::fs::read_dir(&shipped)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "maps" || name == "scenarios" {
            std::fs::create_dir(root.join(&name))?;
            for inner in std::fs::read_dir(entry.path())? {
                let inner = inner?;
                std::os::unix::fs::symlink(inner.path(), root.join(&name).join(inner.file_name()))?;
            }
        } else {
            std::os::unix::fs::symlink(entry.path(), root.join(&name))?;
        }
    }
    let map = std::fs::read_to_string(shipped.join("maps/showcase_canyon.scmap.ron"))?;
    std::fs::write(root.join("maps/minimap_tall.scmap.ron"), taller(&map)?)?;
    let scenario = std::fs::read_to_string(shipped.join(SKIRMISH_SCENARIO))?.replace(
        "maps/showcase_canyon.scmap.ron",
        "maps/minimap_tall.scmap.ron",
    );
    std::fs::write(root.join("scenarios/minimap_tall.ron"), scenario)?;
    Ok("scenarios/minimap_tall.ron".to_owned())
}

/// `map` with [`EXTRA_ROWS`] more rows: each row layer's first row repeated before it, ramps
/// none. The file lists rows from the top (`y = height − 1`) down, so rows added first keep every
/// authored cell, start location and resource at its coordinates.
fn taller(map: &str) -> TestResult<String> {
    let mut out = String::new();
    let mut layer: Option<&str> = None;
    for line in map.lines() {
        let trimmed = line.trim();
        if trimmed == "height: 64," {
            out.push_str(&line.replace("64", &(64 + EXTRA_ROWS).to_string()));
            out.push('\n');
            continue;
        }
        if let Some(name) = layer.take() {
            if trimmed.starts_with('"') {
                let row = if name == "ramps" {
                    line.replace(|c: char| c != '"' && c != ',' && c != ' ', ".")
                } else {
                    line.to_owned()
                };
                for _ in 0..EXTRA_ROWS {
                    out.push_str(&row);
                    out.push('\n');
                }
            }
        }
        for name in ["levels", "terrain", "ramps", "flags"] {
            if trimmed == format!("{name}: [") {
                layer = Some(name);
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !out.contains(&format!("height: {},", 64 + EXTRA_ROWS)) {
        return Err("the skirmish map is not 64 rows high".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The child
// ---------------------------------------------------------------------------------------------

/// What the child is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    /// The battle has not published its second tick, or the minimap has no layout yet.
    #[default]
    Boot,
    /// The clock is stopped; the node has drawn `since` frames when this phase began.
    Drawing { since: u64 },
    /// The texture was requested.
    Reading,
    /// The texture is written; the window shot was requested.
    Shooting,
    /// Everything is written.
    Done,
}

/// The child's staging.
#[derive(Resource, Default)]
struct Staging {
    directory: PathBuf,
    phase: Phase,
    shot: MinimapShot,
}

/// Advances the child by one frame.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn stage(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, Staging>,
    session: Res<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    frame: Option<Res<'_, MinimapFrame>>,
    style: Option<Res<'_, MinimapStyle>>,
    terrain: Option<Res<'_, TerrainState>>,
    geometry: Res<'_, HudGeometry>,
    evidence: Res<'_, MinimapRenderEvidence>,
) {
    let tick = session.published().map_or(0, |published| published.tick);
    staging.phase = match staging.phase {
        Phase::Boot => {
            let ready = session.is_ready()
                && tick >= 2
                && frame.as_ref().is_some_and(|frame| frame.layout().is_some())
                && geometry.minimap.is_some();
            if ready {
                lab.battle_paused = true;
                Phase::Drawing {
                    since: evidence.frames(),
                }
            } else {
                Phase::Boot
            }
        }
        Phase::Drawing { since } if evidence.frames() < since + DRAWN_FRAMES => {
            Phase::Drawing { since }
        }
        Phase::Drawing { .. } => {
            let (frame, style, terrain) = (
                frame.expect("the minimap exists"),
                style.expect("the style loaded"),
                terrain.expect("the battle's terrain"),
            );
            record(&mut staging.shot, &frame, &style, &terrain, &geometry);
            staging.shot.drawn = evidence.frames();
            read_texture(&mut commands, &staging.directory, &frame);
            Phase::Reading
        }
        phase => phase,
    };
}

/// Fills the report from the frame, the style, the map and the HUD.
fn record(
    shot: &mut MinimapShot,
    frame: &MinimapFrame,
    style: &MinimapStyle,
    terrain: &TerrainState,
    geometry: &HudGeometry,
) {
    let layout = frame.layout().expect("a layout");
    let square = geometry.minimap.expect("the minimap is drawn");
    *shot = MinimapShot {
        side: frame.side_px(),
        offset: layout.offset.to_array(),
        scale: layout.scale,
        map: [terrain.map.width(), terrain.map.height()],
        cells: frame.cells().to_vec(),
        kinds: terrain
            .map
            .cells()
            .iter()
            .map(|cell| cell.terrain_kind as u8)
            .collect(),
        unpathable: style.unpathable.srgb,
        background: style.background.srgb,
        hud: [square.left(), square.top(), square.right(), square.bottom()],
        drawn: 0,
    };
}

/// Requests the minimap texture; the observer writes it unpadded and then shoots the window.
fn read_texture(commands: &mut Commands<'_, '_>, directory: &Path, frame: &MinimapFrame) {
    let side = frame.side_px();
    let image = frame.image().expect("the minimap image").clone();
    let directory = directory.to_path_buf();
    commands.spawn(Readback::texture(image)).observe(
        move |trigger: Trigger<'_, ReadbackComplete>,
              mut commands: Commands<'_, '_>,
              mut staging: ResMut<'_, Staging>| {
            if staging.phase != Phase::Reading {
                return;
            }
            let stride = (side * 4).div_ceil(256) * 256;
            let mut pixels = Vec::with_capacity((side * side * 4) as usize);
            for row in 0..side {
                let start = (row * stride) as usize;
                pixels.extend_from_slice(&trigger.event().0[start..start + (side * 4) as usize]);
            }
            image::RgbaImage::from_raw(side, side, pixels)
                .expect("the readback holds side × side pixels")
                .save(directory.join("texture.png"))
                .expect("the texture is written");
            commands.entity(trigger.entity()).despawn();
            shoot_window(&mut commands, &directory);
            staging.phase = Phase::Shooting;
        },
    );
}

/// Shoots the window, then writes the report and exits.
fn shoot_window(commands: &mut Commands<'_, '_>, directory: &Path) {
    let directory = directory.to_path_buf();
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<'_, ScreenshotCaptured>,
              mut staging: ResMut<'_, Staging>,
              mut exit: EventWriter<'_, AppExit>| {
            if staging.phase != Phase::Shooting {
                return;
            }
            trigger
                .event()
                .0
                .clone()
                .try_into_dynamic()
                .expect("a screenshot converts to an image")
                .to_rgba8()
                .save(directory.join("window.png"))
                .expect("the window shot is written");
            let text = ron::to_string(&staging.shot).expect("plain data");
            std::fs::write(directory.join(REPORT_FILE), text).expect("the report is written");
            staging.phase = Phase::Done;
            exit.send(AppExit::Success);
        },
    );
}

/// The child every test above launches.
#[test]
#[ignore = "launched by the tests above on a virtual display"]
fn minimap_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    let scenario = std::env::var(CHILD_SCENARIO_VAR).expect("the parent names a scenario");
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        scenario.as_ref(),
    )
    .expect("the assets must start the client");
    app.insert_resource(Staging {
        directory: Path::new(&directory).to_path_buf(),
        ..Staging::default()
    })
    .add_systems(Update, stage.in_set(BattleSet::Present));
    app.get_sub_app_mut(bevy::render::RenderApp)
        .expect("a real renderer")
        .add_systems(
            bevy::render::Render,
            align_ui_attachment
                .after(bevy::render::view::prepare_view_targets)
                .before(bevy::render::RenderSet::Render),
        );
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// Points the window's egui pass at the camera's output, so a screenshot shows the HUD.
///
/// Bevy 0.15 redirects a screenshotted camera's output to its own attachment, while `bevy_egui`
/// 0.33 draws into the window's swap chain view, so egui never reaches `ScreenshotCaptured`
/// (plan 34's capture amendment). This is the test's copy of `client_diagnostics`'
/// `runtime_capture::ui_attachment`, which a `client` test cannot depend on: it shares the
/// camera's output view with the window entry the egui node reads, and changes nothing else.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn align_ui_attachment(
    cameras: Query<
        '_,
        '_,
        (
            &bevy::render::camera::ExtractedCamera,
            &bevy::render::view::ViewTarget,
        ),
    >,
    mut windows: ResMut<'_, bevy::render::view::ExtractedWindows>,
) {
    for (window_id, window) in &mut windows.windows {
        let target = cameras.iter().find_map(|(camera, target)| {
            matches!(
                &camera.target,
                Some(bevy::render::camera::NormalizedRenderTarget::Window(window))
                    if window.entity() == *window_id
            )
            .then_some(target)
        });
        if let Some(target) = target {
            window.swap_chain_texture_view = Some(target.out_texture().clone());
        }
    }
}
