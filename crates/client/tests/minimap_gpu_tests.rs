//! The minimap on a real GPU: the real shader, the theme's real albedo arrays and the production
//! client (plan `2026-12-01-minimap` §6, G-1 to G-6 and G-3′).
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
//! - G-2, dots: a staged local unit and an enemy in sight show their palette colours at their
//!   mapped centres with a black 1-pixel border, a 5×5 structure covers its footprint ± 1 px,
//!   and in team mode the same pixels are `StarCraft` II's team colours.
//! - G-3, fog: a fogged cell is darker than a visible cell of the same ground by the fog's
//!   darkening, and an enemy in the fog leaves no pixel of its colour.
//! - G-3′, no structure in the terrain: the enemy's start structure, in the fog, is drawn as the
//!   fogged walkable ground around it (Amendment 1, A1.9).
//! - G-4, frustum: the pixels along the camera's predicted trapezoid show `frustum_color`, those
//!   3 px inside do not, and a pan of [`FRUSTUM_PAN`] units moves it by that many units' pixels.
//! - G-5, snapshot and alert: with the staged unseen attacker, its snapshot's square shows its
//!   owner's colour where the image stands, the strike's alert draws a red ring round the struck
//!   unit on its first frames, and no ring is left once its ping ticks are over.
//! - M-S3: over [`STEADY_FRAMES`] frames with the clock running, the renderer creates no buffer,
//!   texture or bind group.
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

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/attack_snapshot_stage.rs"]
mod attack_snapshot_stage;

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use client::battle_hud::HudGeometry;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_pipeline::BattleSet;
use client::minimap::render::MinimapRenderEvidence;
use client::minimap::style::MinimapStyle;
use client::minimap::{ColorMode, MinimapFrame, MinimapSettings};
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
    /// The viewer's knowledge of each cell: 0 unexplored, 1 fogged, 2 visible.
    states: Vec<u8>,
    /// Every mark's centre and half-width, pixels.
    marks: Vec<[f32; 3]>,
    /// `unpathable_color`, sRGB.
    unpathable: [f32; 3],
    /// `background`, sRGB.
    background: [f32; 3],
    /// `fog_darkening` of states 0 (unexplored), 1 (fogged) and 2 (visible).
    darkening: [f32; 3],
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

/// The cell texel of `(x, y)`, its kind and its fog state, or `None` off the map.
fn cell(shot: &MinimapShot, x: i64, y: i64) -> Option<([u8; 4], u8, u8)> {
    let [w, h] = shot.map;
    if x < 0 || y < 0 || x >= i64::from(w) || y >= i64::from(h) {
        return None;
    }
    let index = (y as usize) * w as usize + x as usize;
    let texel = shot.cells[index * 4..index * 4 + 4]
        .try_into()
        .expect("four bytes");
    Some((texel, shot.kinds[index], shot.states[index]))
}

/// Whether cell `(x, y)` and its 3×3 neighbourhood share its texel, kind and fog state, and no
/// mark lies near it: a cell whose centre pixel shows only its own ground, whatever filtering is
/// near.
fn uniform(shot: &MinimapShot, x: u32, y: u32) -> bool {
    let Some(centre) = cell(shot, i64::from(x), i64::from(y)) else {
        return false;
    };
    (-1..=1).all(|dy| {
        (-1..=1).all(|dx| cell(shot, i64::from(x) + dx, i64::from(y) + dy) == Some(centre))
    }) && clear_of_marks(shot, x as f32 + 0.5, y as f32 + 0.5)
}

/// Whether cell `(x, y)` and its 3×3 neighbourhood share one fog state, and no mark lies near
/// it: its centre pixel is the cell's own colour darkened by that state alone.
fn one_fog_state_and_clear(shot: &MinimapShot, x: u32, y: u32) -> bool {
    let state = cell(shot, i64::from(x), i64::from(y)).map(|(_, _, state)| state);
    (-1..=1).all(|dy| {
        (-1..=1)
            .all(|dx| cell(shot, i64::from(x) + dx, i64::from(y) + dy).map(|(_, _, s)| s) == state)
    }) && clear_of_marks(shot, x as f32 + 0.5, y as f32 + 0.5)
}

/// sRGB channel `value` in `[0, 1]` as linear.
fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear channel `value` in `[0, 1]` as sRGB.
fn linear_to_srgb(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// Whether map point `(x, y)` lies more than a pixel clear of every mark's square.
fn clear_of_marks(shot: &MinimapShot, x: f32, y: f32) -> bool {
    let px = shot.offset[0] + x * shot.scale;
    let py = shot.offset[1] + y * shot.scale;
    let margin = 1.0;
    shot.marks
        .iter()
        .all(|[mx, my, half]| (px - mx).abs() > half + margin || (py - my).abs() > half + margin)
}

/// Every cell of the map, as `(x, y)`.
fn every_cell(shot: &MinimapShot) -> impl Iterator<Item = (u32, u32)> {
    let [w, h] = shot.map;
    (0..h).flat_map(move |y| (0..w).map(move |x| (x, y)))
}

/// G-1's per-cell claims: levels brighten, unwalkable ground has its colour, nothing is black.
fn assert_terrain(shot: &MinimapShot, texture: &Texture) {
    // A uniform flat walkable cell of each (level, kind, fog state), by its red byte.
    let mut by_level: std::collections::BTreeMap<(u8, u8, u8), f64> =
        std::collections::BTreeMap::new();
    let mut unpathable = 0;
    for (x, y) in every_cell(shot) {
        let Some((texel, kind, state)) = cell(shot, i64::from(x), i64::from(y)) else {
            continue;
        };
        let pixel = cell_pixel(shot, texture, x, y);
        // An unwalkable cell's colour is one flat value whatever its neighbours' ground: only the
        // fog, sampled bilinearly, and nearby marks could change its centre pixel. Since the
        // terrain shows no structure (Amendment 1, A1.9), most unwalkable ground of the skirmish
        // borders other cells, so it is checked wherever one fog state covers it and no mark is
        // near, against the unwalkable colour darkened by that state.
        if texel[1] == 0 && one_fog_state_and_clear(shot, x, y) {
            let fog = f64::from(shot.darkening[usize::from(state)]);
            for ((channel, colour), background) in
                pixel.iter().zip(shot.unpathable).zip(shot.background)
            {
                let expected = linear_to_srgb(
                    srgb_to_linear(f64::from(colour)) * (1.0 - fog)
                        + srgb_to_linear(f64::from(background)) * fog,
                );
                let got = f64::from(*channel) / 255.0;
                assert!(
                    (got - expected).abs() <= UNPATHABLE_TOLERANCE,
                    "unwalkable cell ({x}, {y}) is {pixel:?}, not {:?}",
                    shot.unpathable
                );
            }
            unpathable += 1;
        } else if texel[1] != 0 && uniform(shot, x, y) && texel[2] == 0 && texel[0] % 85 == 0 {
            by_level.insert((texel[0] / 85, kind, state), luminance(pixel));
        }
    }
    assert!(unpathable > 0, "the map has unwalkable cells to check");
    let mut steps = 0;
    for (&(level, kind, state), &bright) in &by_level {
        let Some(&dim) = level
            .checked_sub(1)
            .and_then(|below| by_level.get(&(below, kind, state)))
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
            if on_map && clear_of_marks(shot, world.x, world.y) {
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
    fog: Option<Res<'_, BattleFog>>,
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
            let fog = fog.expect("the viewer's fog");
            record(&mut staging.shot, &frame, &style, &terrain, &fog, &geometry);
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
    fog: &BattleFog,
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
        states: fog
            .cell_states()
            .iter()
            .map(|state| state_byte(*state))
            .collect(),
        marks: frame
            .marks()
            .iter()
            .map(|mark| [mark.a[0], mark.a[1], mark.b[0]])
            .collect(),
        unpathable: style.unpathable.srgb,
        background: style.background.srgb,
        darkening: [
            style.fog_darkening.unexplored,
            style.fog_darkening.fogged,
            style.fog_darkening.visible,
        ],
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

// ---------------------------------------------------------------------------------------------
// WP2: dots and fog (G-2, G-3, M-S3)
// ---------------------------------------------------------------------------------------------

/// Largest difference per channel, in levels, between a dot and its expected colour.
const DOT_TOLERANCE: u8 = 3;
/// Allowed error of the fog's darkening in linear luminance.
const FOG_TOLERANCE: f64 = 0.03;
/// Frames of the M-S3 window, with the clock running.
const STEADY_FRAMES: u32 = 60;

/// What the marks child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct MarksShot {
    side: u32,
    offset: [f32; 2],
    scale: f32,
    map: [u32; 2],
    cells: Vec<u8>,
    kinds: Vec<u8>,
    /// `VisibilityState` of each cell: 0 unexplored, 1 fogged, 2 visible.
    states: Vec<u8>,
    /// The fog darkening of a fogged cell.
    fogged: f32,
    /// The staged local unit, the enemy in sight and the enemy in fog: map positions.
    local: [f32; 2],
    enemy: [f32; 2],
    hidden: [f32; 2],
    /// The local 5×5 structure's centre and footprint side in cells.
    dome: [f32; 2],
    dome_side: f32,
    /// The enemy's start structure (in the fog): centre and footprint `(w, h)` in cells (G-3′).
    enemy_dome: [f32; 2],
    enemy_dome_size: [f32; 2],
    /// sRGB colours.
    local_color: [f32; 3],
    enemy_color: [f32; 3],
    team_own: [f32; 3],
    team_hostile: [f32; 3],
    border: [f32; 3],
    /// Every mark's centre and half-width, pixels, of the player-mode shot.
    marks: Vec<[f32; 3]>,
    /// The camera frustum's corners in pixels at the player shot, and at the team shot after a
    /// pan of [`FRUSTUM_PAN`] world units along +x (G-4).
    frustum: [[f32; 2]; 4],
    frustum_panned: [[f32; 2]; 4],
    /// `frustum_color`, sRGB.
    frustum_color: [f32; 3],
    /// GPU resources created before and after the steady window (M-S3).
    created_before: [u64; 5],
    created_after: [u64; 5],
    /// Frames the node drew in the steady window.
    steady_drawn: u64,
    /// The minimap pass's GPU time on each frame of the steady window that reported one, in
    /// milliseconds, from Bevy's render diagnostics (plan M-B2).
    pass_gpu_ms: Vec<f64>,
    /// Marks drawn by the pass in the steady window.
    marks_drawn: u64,
}

/// How far the marks child pans the camera between its two shots, world units (G-4).
const FRUSTUM_PAN: f32 = 10.0;

/// Share of the sampled points along the frustum's edges that must show its colour (G-4).
const FRUSTUM_HIT_SHARE: f64 = 0.9;

/// The frame's frustum corners in pixels.
fn frustum_pixels(frame: &MinimapFrame) -> [[f32; 2]; 4] {
    let layout = frame.layout().expect("a layout");
    frame
        .frustum()
        .expect("the frustum is cast")
        .map(|corner| layout.to_minimap(corner).to_array())
}

/// G-4: the pixels along the four edges of `corners` show `colour` at
/// [`FRUSTUM_HIT_SHARE`] of the points sampled on the map, and pixels 3 px inside the
/// trapezoid do not.
fn assert_frustum(shot: &MarksShot, texture: &Texture, corners: [[f32; 2]; 4], what: &str) {
    let corners = corners.map(Vec2::from_array);
    let map_min = Vec2::from_array(shot.offset) + Vec2::splat(2.0);
    let map_max = Vec2::from_array(shot.offset)
        + Vec2::new(shot.map[0] as f32, shot.map[1] as f32) * shot.scale
        - Vec2::splat(2.0);
    let on_map = |p: Vec2| p.cmpge(map_min).all() && p.cmple(map_max).all();
    let centroid = corners.iter().copied().sum::<Vec2>() / 4.0;
    let (mut sampled, mut hits, mut inside_hits, mut inside) = (0_u32, 0_u32, 0_u32, 0_u32);
    for edge in 0..4 {
        let (a, b) = (corners[edge], corners[(edge + 1) % 4]);
        for step in 1..40 {
            let point = a.lerp(b, step as f32 / 40.0);
            if !on_map(point) {
                continue;
            }
            sampled += 1;
            hits += u32::from(is_color(
                texture.at(point.x as u32, point.y as u32),
                shot.frustum_color,
            ));
            let within = point + (centroid - point).normalize_or_zero() * 3.0;
            if on_map(within) {
                inside += 1;
                inside_hits += u32::from(is_color(
                    texture.at(within.x as u32, within.y as u32),
                    shot.frustum_color,
                ));
            }
        }
    }
    eprintln!("G-4 {what}: {hits} of {sampled} edge points, {inside_hits} of {inside} inside");
    assert!(
        sampled >= 20,
        "{what}: enough of the frustum lies on the map"
    );
    assert!(
        f64::from(hits) >= FRUSTUM_HIT_SHARE * f64::from(sampled),
        "{what}: {hits} of {sampled} edge points show the frustum's colour"
    );
    assert!(
        f64::from(inside_hits) < 0.1 * f64::from(inside.max(1)),
        "{what}: {inside_hits} of {inside} points 3 px inside show it"
    );
}

/// Bevy's render diagnostic of the minimap pass's GPU time (the node's `pass_span`).
const MINIMAP_GPU_DIAGNOSTIC: &str = "render/minimap/elapsed_gpu";

/// Runs the marks child and returns its report, both shots and its directory.
fn run_marks_child() -> TestResult<(MarksShot, Texture, Texture, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", MARKS_CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("minimap marks");
    let shot: MarksShot = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    let read = |name: &str| -> TestResult<Texture> {
        let image = image::open(directory.path().join(name))?.to_rgba8();
        Ok(Texture {
            side: image.width(),
            pixels: image.into_raw(),
        })
    };
    let (player, team) = (read("player.png")?, read("team.png")?);
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        let keep = Path::new(&keep).join("minimap_marks");
        std::fs::create_dir_all(&keep)?;
        for file in ["player.png", "team.png", REPORT_FILE] {
            std::fs::copy(directory.path().join(file), keep.join(file))?;
        }
    }
    Ok((shot, player, team, directory))
}

/// The snapped pixel of map point `world`.
fn dot_pixel(shot: &MarksShot, world: [f32; 2]) -> (u32, u32) {
    (
        (shot.offset[0] + world[0] * shot.scale).floor() as u32,
        (shot.offset[1] + world[1] * shot.scale).floor() as u32,
    )
}

/// Whether `pixel` is `color` (sRGB in [0, 1]) within [`DOT_TOLERANCE`] levels per channel.
fn is_color(pixel: [u8; 4], color: [f32; 3]) -> bool {
    (0..3).all(|c| pixel[c].abs_diff((color[c] * 255.0).round() as u8) <= DOT_TOLERANCE)
}

/// G-2, G-3 and M-S3 from one child: dots in player and team colours with black borders, the
/// 5×5 structure's square, the fog's darkening, no dot for an enemy in the fog, and no GPU
/// resource created in a running steady state.
#[test]
fn g2_g3_dots_fog_and_a_steady_gpu_state_on_the_real_shader() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let (shot, player, team, _directory) = run_marks_child()?;
    // G-2: centres in the owners' colours, then S8's team colours.
    for (world, color, team_color, what) in [
        (
            shot.local,
            shot.local_color,
            shot.team_own,
            "the local unit",
        ),
        (
            shot.enemy,
            shot.enemy_color,
            shot.team_hostile,
            "the enemy in sight",
        ),
    ] {
        let (x, y) = dot_pixel(&shot, world);
        let (seen, seen_team) = (player.at(x, y), team.at(x, y));
        assert!(
            is_color(seen, color),
            "{what} at ({x}, {y}) is {seen:?}, not {color:?}"
        );
        assert!(
            is_color(seen_team, team_color),
            "{what} in team mode is {seen_team:?}, not {team_color:?}"
        );
        // The 3-pixel dot's 1-pixel border, on each side.
        for (bx, by) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            let border = player.at(bx, by);
            assert!(
                is_color(border, shot.border),
                "{what}'s border ({bx}, {by}) is {border:?}"
            );
        }
    }
    // The 5×5 structure covers its mapped footprint ± 1 px: its corners are border, one pixel
    // further out is neither border nor its colour.
    let (cx, cy) = dot_pixel(&shot, shot.dome);
    let half = shot.dome_side * 0.5 * shot.scale;
    // A corner another square covers (a worker beside the structure) proves nothing.
    let covered_by_other = |(x, y): (u32, u32)| {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        shot.marks.iter().any(|[mx, my, mhalf]| {
            (mx - (cx as f32 + 0.5)).abs() > 0.1
                && (px - mx).abs() <= mhalf + 1.0
                && (py - my).abs() <= mhalf + 1.0
        })
    };
    let mut corners = 0;
    for (sx, sy) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let inside = (
            (cx as f32 + 0.5 + sx * (half - 1.0)).floor() as u32,
            (cy as f32 + 0.5 + sy * (half - 1.0)).floor() as u32,
        );
        let outside = (
            (cx as f32 + 0.5 + sx * (half + 1.0)).floor() as u32,
            (cy as f32 + 0.5 + sy * (half + 1.0)).floor() as u32,
        );
        if covered_by_other(inside) || covered_by_other(outside) {
            continue;
        }
        corners += 1;
        let (inner, outer) = (
            player.at(inside.0, inside.1),
            player.at(outside.0, outside.1),
        );
        assert!(
            is_color(inner, shot.border) || is_color(inner, shot.local_color),
            "the structure's corner {inside:?} is {inner:?}"
        );
        assert!(
            !is_color(outer, shot.border) && !is_color(outer, shot.local_color),
            "one pixel past the structure's corner {outside:?} is {outer:?}"
        );
    }
    assert!(
        corners >= 2,
        "at least two of the structure's corners are clear to check"
    );
    // G-3: no pixel of the enemy's colour where the enemy in the fog stands.
    let (hx, hy) = dot_pixel(&shot, shot.hidden);
    for y in hy.saturating_sub(3)..=hy + 3 {
        for x in hx.saturating_sub(3)..=hx + 3 {
            assert!(
                !is_color(player.at(x, y), shot.enemy_color),
                "the enemy in the fog shows at ({x}, {y})"
            );
        }
    }
    assert_fog_darkens(&shot, &player);
    assert_no_structure_in_terrain(&shot, &player);
    // G-4: the frustum at both shots; the pan moved it by the pan times the scale.
    assert_frustum(&shot, &player, shot.frustum, "before the pan");
    assert_frustum(&shot, &team, shot.frustum_panned, "after the pan");
    for (before, after) in shot.frustum.iter().zip(shot.frustum_panned) {
        let moved = Vec2::from_array(after) - Vec2::from_array(*before);
        assert!(
            (moved - Vec2::new(FRUSTUM_PAN * shot.scale, 0.0)).length() < 0.5,
            "a corner moved by {moved}, not {} px along x",
            FRUSTUM_PAN * shot.scale
        );
    }
    // M-S3: the running steady window created nothing on the GPU.
    eprintln!(
        "GPU resources created: {:?} before, {:?} after {} drawn frames",
        shot.created_before, shot.created_after, shot.steady_drawn
    );
    assert!(
        shot.steady_drawn >= u64::from(STEADY_FRAMES) / 2,
        "the node drew the window"
    );
    assert_eq!(
        shot.created_before, shot.created_after,
        "no GPU resource is recreated"
    );
    // M-B2's figure, reported, not asserted: CI's software Vulkan is far slower than the bar.
    let samples = shot.pass_gpu_ms.len();
    let mean = shot.pass_gpu_ms.iter().sum::<f64>() / samples.max(1) as f64;
    let mut sorted = shot.pass_gpu_ms.clone();
    sorted.sort_by(f64::total_cmp);
    eprintln!(
        "M-B2 minimap pass GPU time on {}: mean {mean:.4} ms, median {:.4} ms, max {:.4} ms over \
         {samples} frames, {} marks",
        gpu_run::selected_driver().driver,
        sorted.get(samples / 2).copied().unwrap_or(f64::NAN),
        sorted.last().copied().unwrap_or(f64::NAN),
        shot.marks_drawn
    );
    Ok(())
}

/// G-3: a fogged cell is darker than a visible cell of the same ground by the fog's darkening.
fn assert_fog_darkens(shot: &MarksShot, texture: &Texture) {
    let [w, h] = shot.map;
    let index = |x: u32, y: u32| (y * w + x) as usize;
    let clear_of_marks = |x: u32, y: u32| {
        let px = shot.offset[0] + (x as f32 + 0.5) * shot.scale;
        let py = shot.offset[1] + (y as f32 + 0.5) * shot.scale;
        shot.marks.iter().all(|[mx, my, half]| {
            (px - mx).abs() > half + 2.0 * shot.scale || (py - my).abs() > half + 2.0 * shot.scale
        })
    };
    // A uniform cell: its 3×3 neighbourhood shares its texel, kind and fog state.
    let uniform = |x: u32, y: u32| {
        x > 0
            && y > 0
            && x + 1 < w
            && y + 1 < h
            && (y - 1..=y + 1).all(|ny| {
                (x - 1..=x + 1).all(|nx| {
                    let (a, b) = (index(x, y), index(nx, ny));
                    shot.cells[a * 4..a * 4 + 4] == shot.cells[b * 4..b * 4 + 4]
                        && shot.kinds[a] == shot.kinds[b]
                        && shot.states[a] == shot.states[b]
                })
            })
    };
    let mut lit: std::collections::BTreeMap<(Vec<u8>, u8), f64> = std::collections::BTreeMap::new();
    let mut dark: std::collections::BTreeMap<(Vec<u8>, u8), f64> =
        std::collections::BTreeMap::new();
    for y in 0..h {
        for x in 0..w {
            let i = index(x, y);
            let texel = shot.cells[i * 4..i * 4 + 4].to_vec();
            if texel[1] == 0 || !uniform(x, y) || !clear_of_marks(x, y) {
                continue;
            }
            let px = shot.offset[0] + (x as f32 + 0.5) * shot.scale;
            let py = shot.offset[1] + (y as f32 + 0.5) * shot.scale;
            let value = luminance(texture.at(px as u32, py as u32));
            match shot.states[i] {
                2 => lit.insert((texel, shot.kinds[i]), value),
                1 => dark.insert((texel, shot.kinds[i]), value),
                _ => None,
            };
        }
    }
    let pairs: Vec<(f64, f64)> = lit
        .iter()
        .filter_map(|(key, value)| dark.get(key).map(|fogged| (*value, *fogged)))
        .collect();
    assert!(!pairs.is_empty(), "some ground is both visible and fogged");
    for (visible, fogged) in pairs {
        let darkening = 1.0 - fogged / visible;
        eprintln!("fog darkens {visible:.4} to {fogged:.4}: {darkening:.3}");
        assert!(
            (darkening - f64::from(shot.fogged)).abs() <= FOG_TOLERANCE,
            "darkening {darkening:.3} is not {} ± {FOG_TOLERANCE}",
            shot.fogged
        );
    }
}

/// G-3′ (Amendment 1, A1.9): the enemy's start structure, in the fog, leaves no trace in the
/// terrain. The pixel at its mapped centre is within [`FOG_TOLERANCE`] linear luminance of the
/// nearest fogged walkable cell of the same level and ground kind outside its footprint, not the
/// unwalkable colour darkened.
fn assert_no_structure_in_terrain(shot: &MarksShot, texture: &Texture) {
    let [w, h] = shot.map;
    let index = |x: u32, y: u32| (y * w + x) as usize;
    let [cx, cy] = shot.enemy_dome;
    let (centre_x, centre_y) = (cx.floor() as u32, cy.floor() as u32);
    let centre = index(centre_x, centre_y);
    assert_eq!(
        shot.states[centre], 1,
        "the enemy's start structure is in the fog"
    );
    let [fw, fh] = shot.enemy_dome_size;
    let inside = |x: u32, y: u32| {
        (x as f32 + 0.5 - cx).abs() < fw * 0.5 + 1.0 && (y as f32 + 0.5 - cy).abs() < fh * 0.5 + 1.0
    };
    let clear_of_marks = |x: u32, y: u32| {
        let px = shot.offset[0] + (x as f32 + 0.5) * shot.scale;
        let py = shot.offset[1] + (y as f32 + 0.5) * shot.scale;
        shot.marks.iter().all(|[mx, my, half]| {
            (px - mx).abs() > half + 2.0 * shot.scale || (py - my).abs() > half + 2.0 * shot.scale
        })
    };
    let level = shot.cells[centre * 4];
    let kind = shot.kinds[centre];
    let reference = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let i = index(x, y);
            !inside(x, y)
                && shot.states[i] == 1
                && shot.cells[i * 4] == level
                && shot.cells[i * 4 + 1] == u8::MAX
                && shot.cells[i * 4 + 2] == 0
                && shot.kinds[i] == kind
                && clear_of_marks(x, y)
        })
        .min_by_key(|&(x, y)| {
            let (dx, dy) = (
                i64::from(x) - i64::from(centre_x),
                i64::from(y) - i64::from(centre_y),
            );
            dx * dx + dy * dy
        })
        .expect("fogged walkable ground of the structure's level lies near it");
    let pixel = |x: f32, y: f32| {
        luminance(texture.at(
            (shot.offset[0] + x * shot.scale) as u32,
            (shot.offset[1] + y * shot.scale) as u32,
        ))
    };
    let under = pixel(cx, cy);
    let ground = pixel(reference.0 as f32 + 0.5, reference.1 as f32 + 0.5);
    eprintln!(
        "G-3′: the enemy's start structure's centre {under:.4}, fogged ground at {reference:?} \
         {ground:.4}"
    );
    assert!(
        (under - ground).abs() <= FOG_TOLERANCE,
        "the enemy's start structure shows in the terrain: {under:.4} against fogged ground \
         {ground:.4} ± {FOG_TOLERANCE}"
    );
}

/// Name of the libtest entry of the marks child.
const MARKS_CHILD_TEST: &str = "minimap_marks_child";

/// What the marks child is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MarksPhase {
    #[default]
    Boot,
    /// Units are staged; the clock runs until tick `until` is published.
    Settling {
        until: u32,
    },
    /// The clock is stopped; shot `shot` (0 player, 1 team) waits for the node to draw.
    Drawing {
        shot: usize,
        since: u64,
    },
    /// Shot `shot` was requested.
    Reading {
        shot: usize,
    },
    /// The clock runs for the steady window; `frames` counted so far.
    Steady {
        frames: u32,
        since: u64,
    },
    Done,
}

/// The marks child's staging.
#[derive(Resource, Default)]
struct MarksStaging {
    directory: PathBuf,
    phase: MarksPhase,
    shot: MarksShot,
    /// The staged local unit, the enemy in sight and the enemy in the fog.
    staged: Option<[sim_types::EntityId; 3]>,
}

/// Fog state of `cell` as the report's byte.
fn state_byte(state: sim_types::VisibilityState) -> u8 {
    match state {
        sim_types::VisibilityState::Unexplored => 0,
        sim_types::VisibilityState::Fogged => 1,
        sim_types::VisibilityState::Visible => 2,
    }
}

/// A walkable, flat cell centre whose `reach`-cell neighbourhood is all in `state`, at least
/// `clear` world units from every presented bubble and from `taken`.
fn isolated_cell(
    world: &sim_core::SimWorld,
    fog: &BattleFog,
    bubbles: &[sim_types::BubbleRenderInstance],
    state: sim_types::VisibilityState,
    reach: i64,
    clear: f32,
    taken: &[Vec2],
) -> Option<Vec2> {
    let map = &world.map;
    let (w, h) = (i64::from(map.width()), i64::from(map.height()));
    let states = fog.cell_states();
    let open = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && x < w
            && y < h
            && map.cell(x as u32, y as u32).is_ok_and(|cell| {
                cell.ramp.is_none()
                    && cell.flags.contains(sim_map::CellFlags::GROUND_PATHABLE)
                    && states.get((y * w + x) as usize) == Some(&state)
            })
    };
    (reach..h - reach)
        .flat_map(|y| (reach..w - reach).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            (-reach..=reach).all(|dy| (-reach..=reach).all(|dx| open(x + dx, y + dy)))
        })
        .map(|(x, y)| Vec2::new(x as f32 + 0.5, y as f32 + 0.5))
        .find(|point| {
            bubbles
                .iter()
                .all(|b| point.distance(Vec2::new(b.world_pos[0], b.world_pos[1])) >= clear)
                && taken.iter().all(|other| point.distance(*other) >= clear)
        })
}

/// Spawns the local unit and an enemy in sight, and an enemy in the fog, each of the local
/// workers' type, at isolated cells. Returns them, or `None` when no such cells exist.
fn stage_units(session: &mut BattleSession, fog: &BattleFog) -> Option<[sim_types::EntityId; 3]> {
    use sim_types::VisibilityState::{Fogged, Visible};
    let viewer = fog.viewer()?;
    let bubbles = session.frames.presented().bubbles.clone();
    let world = session.world_mut()?;
    let worker = world
        .entities
        .values()
        .find(|unit| unit.owner == viewer && !unit.is_immobile)?
        .unit_type_id;
    let enemy = world
        .entities
        .values()
        .map(|unit| unit.owner)
        .find(|owner| *owner != viewer && *owner != sim_types::PlayerId::NEUTRAL)?;
    let local_at = isolated_cell(world, fog, &bubbles, Visible, 2, 4.0, &[])?;
    let enemy_at = isolated_cell(world, fog, &bubbles, Visible, 2, 4.0, &[local_at])?;
    let hidden_at = isolated_cell(world, fog, &bubbles, Fogged, 4, 6.0, &[])?;
    let at = |point: Vec2| {
        sim_types::SimVec2::new(
            sim_types::SimScalar::from_num(point.x),
            sim_types::SimScalar::from_num(point.y),
        )
    };
    Some([
        world.spawn_unit(worker, viewer, at(local_at)).ok()?,
        world.spawn_unit(worker, enemy, at(enemy_at)).ok()?,
        world.spawn_unit(worker, enemy, at(hidden_at)).ok()?,
    ])
}

/// The presented map position of `id`.
fn presented_at(session: &BattleSession, id: sim_types::EntityId) -> [f32; 2] {
    let bubble = session
        .frames
        .presented()
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == id)
        .expect("the staged unit is presented");
    [bubble.world_pos[0], bubble.world_pos[1]]
}

/// Fills the marks report from the frame, the style, the fog, the palette and the staging.
#[allow(
    clippy::too_many_arguments,
    reason = "one argument per source of the report"
)]
fn record_marks(
    shot: &mut MarksShot,
    staged: [sim_types::EntityId; 3],
    frame: &MinimapFrame,
    style: &MinimapStyle,
    fog: &BattleFog,
    terrain: &TerrainState,
    session: &BattleSession,
    palette: &client::player_presentation::PlayerPalette,
) {
    let layout = frame.layout().expect("a layout");
    let viewer = fog.viewer().expect("a viewer");
    let enemy = session
        .world()
        .and_then(|world| world.entities.get(&staged[1]))
        .expect("the staged enemy lives")
        .owner;
    let dome = session
        .frames
        .presented()
        .bubbles
        .iter()
        .find(|bubble| bubble.owner == viewer && bubble.shape.footprint() == Some((5, 5)))
        .expect("the local player has a 5×5 structure");
    let enemy_dome = session
        .frames
        .presented()
        .bubbles
        .iter()
        .find(|bubble| bubble.owner == enemy && bubble.shape.footprint().is_some())
        .expect("the enemy has a start structure");
    let (enemy_w, enemy_h) = enemy_dome.shape.footprint().expect("a footprint");
    let srgb = |owner| palette.resolve(owner).expect("a palette owner").srgb;
    *shot = MarksShot {
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
        states: fog
            .cell_states()
            .iter()
            .map(|state| state_byte(*state))
            .collect(),
        fogged: style.fog_darkening.fogged,
        local: presented_at(session, staged[0]),
        enemy: presented_at(session, staged[1]),
        hidden: presented_at(session, staged[2]),
        dome: [dome.world_pos[0], dome.world_pos[1]],
        dome_side: 5.0,
        enemy_dome: [enemy_dome.world_pos[0], enemy_dome.world_pos[1]],
        enemy_dome_size: [f32::from(enemy_w), f32::from(enemy_h)],
        local_color: srgb(viewer),
        enemy_color: srgb(enemy),
        team_own: style.team.own.srgb,
        team_hostile: style.team.hostile.srgb,
        border: style.border.srgb,
        frustum: frustum_pixels(frame),
        frustum_color: style.frustum.srgb,
        marks: frame
            .marks()
            .iter()
            .map(|mark| [mark.a[0], mark.a[1], mark.b[0]])
            .collect(),
        ..MarksShot::default()
    };
}

/// What the marks child reads to stage and record.
type MarksChildSources<'w> = (
    Option<Res<'w, MinimapFrame>>,
    Option<Res<'w, MinimapStyle>>,
    Option<Res<'w, TerrainState>>,
    Option<Res<'w, BattleFog>>,
    Option<Res<'w, client::player_presentation::PlayerPalette>>,
);

/// Advances the marks child by one frame.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn stage_marks(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, MarksStaging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    mut exit: EventWriter<'_, AppExit>,
    sources: MarksChildSources<'_>,
    evidence: Res<'_, MinimapRenderEvidence>,
    diagnostics: Res<'_, bevy::diagnostic::DiagnosticsStore>,
) {
    let (frame, style, terrain, fog, palette) = sources;
    let tick = session.published().map_or(0, |published| published.tick);
    let staging = staging.as_mut();
    staging.phase = match staging.phase {
        MarksPhase::Boot => {
            let ready = session.is_ready()
                && tick >= 2
                && frame.as_ref().is_some_and(|frame| frame.layout().is_some())
                && fog
                    .as_ref()
                    .is_some_and(|fog| !fog.cell_states().is_empty());
            match fog.filter(|_| ready) {
                Some(fog) => {
                    staging.staged = Some(
                        stage_units(&mut session, &fog).expect("the skirmish has isolated cells"),
                    );
                    lab.battle_paused = false;
                    MarksPhase::Settling { until: tick + 3 }
                }
                None => MarksPhase::Boot,
            }
        }
        MarksPhase::Settling { until } if tick < until => MarksPhase::Settling { until },
        MarksPhase::Settling { .. } => {
            lab.battle_paused = true;
            MarksPhase::Drawing {
                shot: 0,
                since: evidence.frames(),
            }
        }
        MarksPhase::Drawing { shot, since } if evidence.frames() < since + DRAWN_FRAMES => {
            MarksPhase::Drawing { shot, since }
        }
        MarksPhase::Drawing { shot, .. } => {
            let frame = frame.expect("the minimap exists");
            if shot == 1 {
                staging.shot.frustum_panned = frustum_pixels(&frame);
            }
            if shot == 0 {
                record_marks(
                    &mut staging.shot,
                    staging.staged.expect("staged"),
                    &frame,
                    &style.expect("the style"),
                    &fog.expect("the fog"),
                    &terrain.expect("the terrain"),
                    &session,
                    &palette.expect("the palette"),
                );
            }
            read_shot(&mut commands, &staging.directory, &frame, shot);
            MarksPhase::Reading { shot }
        }
        // The readback's observer moves the phase on.
        MarksPhase::Reading { shot } => MarksPhase::Reading { shot },
        MarksPhase::Steady { frames, since } if frames < STEADY_FRAMES => {
            if let Some(ms) = diagnostics
                .get(&bevy::diagnostic::DiagnosticPath::const_new(
                    MINIMAP_GPU_DIAGNOSTIC,
                ))
                .and_then(bevy::diagnostic::Diagnostic::value)
            {
                staging.shot.pass_gpu_ms.push(ms);
            }
            MarksPhase::Steady {
                frames: frames + 1,
                since,
            }
        }
        MarksPhase::Steady { since, .. } => {
            staging.shot.created_after = evidence.created();
            staging.shot.steady_drawn = evidence.frames() - since;
            staging.shot.marks_drawn = evidence.drawn_marks();
            let text = ron::to_string(&staging.shot).expect("plain data");
            std::fs::write(staging.directory.join(REPORT_FILE), text)
                .expect("the report is written");
            exit.send(AppExit::Success);
            MarksPhase::Done
        }
        MarksPhase::Done => MarksPhase::Done,
    };
}

/// Requests shot `shot` of the minimap texture; the observer writes it and moves on: after the
/// player shot to team colours, after the team shot to the running steady window.
fn read_shot(commands: &mut Commands<'_, '_>, directory: &Path, frame: &MinimapFrame, shot: usize) {
    let side = frame.side_px();
    let image = frame.image().expect("the minimap image").clone();
    let path = directory.join(if shot == 0 { "player.png" } else { "team.png" });
    commands.spawn(Readback::texture(image)).observe(
        move |trigger: Trigger<'_, ReadbackComplete>,
              mut commands: Commands<'_, '_>,
              mut staging: ResMut<'_, MarksStaging>,
              mut settings: ResMut<'_, MinimapSettings>,
              mut lab: ResMut<'_, LabState>,
              mut cameras: Query<'_, '_, &mut client::RtsCamera>,
              evidence: Res<'_, MinimapRenderEvidence>| {
            if staging.phase != (MarksPhase::Reading { shot }) {
                return;
            }
            save_unpadded(&trigger.event().0, side, &path);
            commands.entity(trigger.entity()).despawn();
            staging.phase = if shot == 0 {
                settings.color_mode = ColorMode::Team;
                for mut camera in &mut cameras {
                    camera.target.x += FRUSTUM_PAN;
                }
                MarksPhase::Drawing {
                    shot: 1,
                    since: evidence.frames(),
                }
            } else {
                staging.shot.created_before = evidence.created();
                lab.battle_paused = false;
                MarksPhase::Steady {
                    frames: 0,
                    since: evidence.frames(),
                }
            };
        },
    );
}

/// Writes a readback of a `side` square, rows padded to 256 bytes, as a PNG.
fn save_unpadded(data: &[u8], side: u32, path: &Path) {
    let stride = (side * 4).div_ceil(256) * 256;
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    for row in 0..side {
        let start = (row * stride) as usize;
        pixels.extend_from_slice(&data[start..start + (side * 4) as usize]);
    }
    image::RgbaImage::from_raw(side, side, pixels)
        .expect("the readback holds side × side pixels")
        .save(path)
        .expect("the texture is written");
}

/// The child of [`g2_g3_dots_fog_and_a_steady_gpu_state_on_the_real_shader`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn minimap_marks_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    app.insert_resource(MarksStaging {
        directory: Path::new(&directory).to_path_buf(),
        ..MarksStaging::default()
    })
    .add_systems(Update, stage_marks.in_set(BattleSet::Present));
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

// ---------------------------------------------------------------------------------------------
// G-5: the attacker snapshot's square and the alert's ring on the real shader.
// ---------------------------------------------------------------------------------------------

/// Name of the libtest entry of the alert child.
const ALERT_CHILD_TEST: &str = "minimap_alert_child";

/// Angles sampled round a ring.
const RING_SAMPLES: u32 = 24;

/// What the alert child saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct AlertShot {
    offset: [f32; 2],
    scale: f32,
    /// The snapshot image's map position and its owner's sRGB colour.
    snapshot: [f32; 2],
    snapshot_color: [f32; 3],
    /// The alert ping's centre in pixels, its start tick and the presented tick of each shot.
    ping_px: [f32; 2],
    ping_tick: u32,
    ping_ticks: u32,
    shot_ticks: [u32; 2],
    /// Ring radius at start and end, ring width, in pixels; the ping's sRGB colour.
    ring_radius: [f32; 2],
    ring_px: f32,
    ping_color: [f32; 3],
}

/// The ring's radius in pixels at `tick`, as the shader grows it.
fn ring_radius_at(shot: &AlertShot, tick: u32) -> f32 {
    let t = (tick.saturating_sub(shot.ping_tick) as f32 / shot.ping_ticks as f32).clamp(0.0, 1.0);
    shot.ring_radius[0] + (shot.ring_radius[1] - shot.ring_radius[0]) * t
}

/// How many of the [`RING_SAMPLES`] points on the circle of `radius` round the ping show its
/// colour.
fn ring_hits(shot: &AlertShot, texture: &Texture, radius: f32) -> u32 {
    (0..RING_SAMPLES)
        .filter(|step| {
            let angle = *step as f32 / RING_SAMPLES as f32 * std::f32::consts::TAU;
            let x = shot.ping_px[0] + radius * angle.cos();
            let y = shot.ping_px[1] + radius * angle.sin();
            let side = texture.side as f32;
            (0.0..side).contains(&x)
                && (0.0..side).contains(&y)
                && is_color(texture.at(x as u32, y as u32), shot.ping_color)
        })
        .count() as u32
}

/// G-5: with the staged unseen attacker, its snapshot's square shows its owner's colour where the
/// image stands; the strike's alert draws a red ring round the struck unit on its first frames,
/// and no ring once its ping ticks are over.
#[test]
fn g5_the_snapshot_and_the_alert_ring_on_the_real_shader() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", ALERT_CHILD_TEST]);
    command.env(CHILD_OUT_VAR, directory.path());
    command.allow_for_hex_tiling_driver_defect();
    command.assert_passes_validation("minimap alert");
    let shot: AlertShot = ron::from_str(&std::fs::read_to_string(
        directory.path().join(REPORT_FILE),
    )?)?;
    let read = |name: &str| -> TestResult<Texture> {
        let image = image::open(directory.path().join(name))?.to_rgba8();
        Ok(Texture {
            side: image.width(),
            pixels: image.into_raw(),
        })
    };
    let (first, late) = (read("ring.png")?, read("faded.png")?);
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        let keep = Path::new(&keep).join("minimap_alert");
        std::fs::create_dir_all(&keep)?;
        for file in ["ring.png", "faded.png", REPORT_FILE] {
            std::fs::copy(directory.path().join(file), keep.join(file))?;
        }
    }
    let x = (shot.offset[0] + shot.snapshot[0] * shot.scale).floor() as u32;
    let y = (shot.offset[1] + shot.snapshot[1] * shot.scale).floor() as u32;
    let seen = first.at(x, y);
    assert!(
        is_color(seen, shot.snapshot_color),
        "the snapshot's square at ({x}, {y}) is {seen:?}, not {:?}",
        shot.snapshot_color
    );
    let radius = ring_radius_at(&shot, shot.shot_ticks[0]);
    let hits = ring_hits(&shot, &first, radius);
    eprintln!("G-5: {hits} of {RING_SAMPLES} ring points at radius {radius:.2} px");
    assert!(
        hits >= RING_SAMPLES * 3 / 4,
        "the ring shows on its first frames: {hits} of {RING_SAMPLES}"
    );
    assert!(
        shot.shot_ticks[1] >= shot.ping_tick + shot.ping_ticks,
        "the late shot is after the ping"
    );
    let mut late_hits = 0;
    for step in 0..=20 {
        let radius =
            shot.ring_radius[0] + (shot.ring_radius[1] - shot.ring_radius[0]) * step as f32 / 20.0;
        late_hits += ring_hits(&shot, &late, radius);
    }
    assert_eq!(late_hits, 0, "no ring after {} ticks", shot.ping_ticks);
    Ok(())
}

/// What the alert child is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AlertPhase {
    #[default]
    Boot,
    /// The staged fight runs until the snapshot and the alert's ping exist.
    Fighting,
    /// The clock is stopped; shot `shot` waits for the node to draw.
    Drawing {
        shot: usize,
        since: u64,
    },
    /// Shot `shot` was requested.
    Reading {
        shot: usize,
    },
    /// The clock runs until tick `until`, past the ping.
    Fading {
        until: u32,
    },
    Done,
}

/// The alert child's staging.
#[derive(Resource, Default)]
struct AlertStaging {
    directory: PathBuf,
    phase: AlertPhase,
    shot: AlertShot,
}

/// What the alert child reads.
type AlertChildSources<'w> = (
    Option<Res<'w, MinimapFrame>>,
    Option<Res<'w, MinimapStyle>>,
    Option<Res<'w, BattleFog>>,
    Option<Res<'w, client::minimap::alerts::AlertLog>>,
    Option<Res<'w, client::player_presentation::PlayerPalette>>,
);

/// Fills the alert report's first part once the snapshot and the ping exist.
fn record_alert(
    shot: &mut AlertShot,
    frame: &MinimapFrame,
    style: &MinimapStyle,
    fog: &BattleFog,
    log: &client::minimap::alerts::AlertLog,
    palette: &client::player_presentation::PlayerPalette,
    tick: u32,
) {
    let layout = frame.layout().expect("a layout");
    let image = &fog.attack_snapshots()[0].bubble;
    let ping = log.pings()[0];
    *shot = AlertShot {
        offset: layout.offset.to_array(),
        scale: layout.scale,
        snapshot: [image.world_pos[0], image.world_pos[1]],
        snapshot_color: palette.resolve(image.owner).expect("a palette owner").srgb,
        ping_px: layout.to_minimap(ping.at).to_array(),
        ping_tick: ping.tick,
        ping_ticks: ping.ticks,
        shot_ticks: [tick, 0],
        ring_radius: [style.alerts.ring_radius_px.0, style.alerts.ring_radius_px.1],
        ring_px: style.alerts.ring_px,
        ping_color: style.alerts.attack.color.srgb,
    };
}

/// Advances the alert child by one frame.
#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    reason = "a Bevy system with one declared parameter per dependency"
)]
fn stage_alert(
    mut commands: Commands<'_, '_>,
    mut staging: ResMut<'_, AlertStaging>,
    mut session: ResMut<'_, BattleSession>,
    mut lab: ResMut<'_, LabState>,
    mut exit: EventWriter<'_, AppExit>,
    sources: AlertChildSources<'_>,
    evidence: Res<'_, MinimapRenderEvidence>,
) {
    let (frame, style, fog, log, palette) = sources;
    let tick = session.published().map_or(0, |published| published.tick);
    let staging = staging.as_mut();
    staging.phase = match staging.phase {
        AlertPhase::Boot => {
            let ready = session.is_ready()
                && tick >= 2
                && frame.as_ref().is_some_and(|frame| frame.layout().is_some());
            if ready {
                let world = session.world_mut().expect("a live world");
                attack_snapshot_stage::stage(world);
                lab.battle_paused = false;
                AlertPhase::Fighting
            } else {
                AlertPhase::Boot
            }
        }
        AlertPhase::Fighting => {
            let (Some(frame), Some(style), Some(fog), Some(log), Some(palette)) =
                (frame, style, fog, log, palette)
            else {
                return;
            };
            if fog.attack_snapshots().is_empty() || log.pings().is_empty() {
                AlertPhase::Fighting
            } else {
                lab.battle_paused = true;
                let presented = session.frames.presented().tick.0;
                record_alert(
                    &mut staging.shot,
                    &frame,
                    &style,
                    &fog,
                    &log,
                    &palette,
                    presented,
                );
                AlertPhase::Drawing {
                    shot: 0,
                    since: evidence.frames(),
                }
            }
        }
        AlertPhase::Drawing { shot, since } if evidence.frames() < since + DRAWN_FRAMES => {
            AlertPhase::Drawing { shot, since }
        }
        AlertPhase::Drawing { shot, .. } => {
            let frame = frame.expect("the minimap exists");
            if shot == 1 {
                staging.shot.shot_ticks[1] = session.frames.presented().tick.0;
            }
            read_alert_shot(&mut commands, &staging.directory, &frame, shot);
            AlertPhase::Reading { shot }
        }
        AlertPhase::Reading { shot } => AlertPhase::Reading { shot },
        AlertPhase::Fading { until } if session.frames.presented().tick.0 < until => {
            AlertPhase::Fading { until }
        }
        AlertPhase::Fading { .. } => {
            lab.battle_paused = true;
            AlertPhase::Drawing {
                shot: 1,
                since: evidence.frames(),
            }
        }
        AlertPhase::Done => {
            exit.send(AppExit::Success);
            AlertPhase::Done
        }
    };
}

/// Requests shot `shot` of the minimap texture; the observer writes it and moves on: after the
/// ring shot to the fading clock, after the faded shot to the report.
fn read_alert_shot(
    commands: &mut Commands<'_, '_>,
    directory: &Path,
    frame: &MinimapFrame,
    shot: usize,
) {
    let side = frame.side_px();
    let image = frame.image().expect("the minimap image").clone();
    let directory = directory.to_path_buf();
    let path = directory.join(if shot == 0 { "ring.png" } else { "faded.png" });
    commands.spawn(Readback::texture(image)).observe(
        move |trigger: Trigger<'_, ReadbackComplete>,
              mut commands: Commands<'_, '_>,
              mut staging: ResMut<'_, AlertStaging>,
              mut lab: ResMut<'_, LabState>| {
            if staging.phase != (AlertPhase::Reading { shot }) {
                return;
            }
            save_unpadded(&trigger.event().0, side, &path);
            commands.entity(trigger.entity()).despawn();
            staging.phase = if shot == 0 {
                lab.battle_paused = false;
                AlertPhase::Fading {
                    until: staging.shot.ping_tick + staging.shot.ping_ticks + 2,
                }
            } else {
                let text = ron::to_string(&staging.shot).expect("plain data");
                std::fs::write(directory.join(REPORT_FILE), text).expect("the report is written");
                AlertPhase::Done
            };
        },
    );
}

/// The child of [`g5_the_snapshot_and_the_alert_ring_on_the_real_shader`].
#[test]
#[ignore = "launched by the test above on a virtual display"]
fn minimap_alert_child() {
    let directory = std::env::var_os(CHILD_OUT_VAR).expect("the parent names a directory");
    let mut app = client::build_graphical_app_off_main_thread(
        ClientMode::Skirmish,
        Some(CHILD_FRAME_LIMIT),
        SKIRMISH_SCENARIO.as_ref(),
    )
    .expect("the shipped assets must start the client");
    app.insert_resource(AlertStaging {
        directory: Path::new(&directory).to_path_buf(),
        ..AlertStaging::default()
    })
    .add_systems(Update, stage_alert.in_set(BattleSet::Present));
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}
