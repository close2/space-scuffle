//! The vertical slice's effects on a real GPU (plan `2026-11-09-effect-visualization`, work
//! package 6b, §7.2, §10.2).
//!
//! # Responsibility
//! For each splash effect the shipped units or the lab-only entries (Amendment 3) name
//! (`support/effect_subjects.rs`, sorted by name; one parent test per index, no effect named
//! in Rust), three children of this binary run
//! the production effect lab (`client::effects::lab`) through `support/gpu_run.rs` (radv with
//! the shared lock, or lavapipe; charter C21) at 1280×720 under the battle camera's default
//! zoom on the bright ground, with the shipped vocabulary and the bindings the unit's weapon
//! publishes. The parent measures the PNGs:
//! - **footprint**: the `Critical` layers alone. A splash's front ends at the projected reach,
//!   within ±2 px on most rays and never beyond it (a front that declares a torn leading edge,
//!   `edge_noise`, may end inside by its declared bite as well, on most rays, while its
//!   farthest clumps stay within ±2 px of the reach: scale-out round 3), and once every non-decal layer has ended
//!   the visible edge of its lasting `Critical` marks (the acid stain, held at full size)
//!   lies on the reach, within ±2 px on most rays and never more than 2 px beyond it (review
//!   round 5; round 4 checked only that it stayed inside); a line's marks start and end at
//!   the line's projected ends within ±2 px;
//! - **coverage**: within 80 ms of the event (of each search point, for a line), at least 60 %
//!   of the projected full-damage disc is changed by the effect;
//! - **no pop**: at the 40 ms step, the mean luminance of the effect's region changes by at
//!   most 6/255 between consecutive frames after the impact frame;
//! - **nothing lingers**: once every non-decal layer has ended only pixels inside the decals'
//!   footprint differ from the empty ground, and after the effect's end none does (2/255);
//! - **golden stills** at 0.08, 0.3 and 0.75 of the effect's lifetime, per driver, in
//!   `tests/data/effects/<driver>/`: pass when ≥ 99 % of the region's pixels are within 8/255
//!   and the mean absolute difference is ≤ 1.5/255. `SPACE_SCUFFLE_EFFECT_GOLDEN_UPDATE=1`
//!   writes them (re-baselining needs the contact sheet in the same commit);
//! - **a line out of the fog** (plan Amendment 2, R6): the line subject seen only from its
//!   fifth search point on draws no pixel at the points before it and its look after it.
//!
//! Plan WP11 (Amendment 24) adds the weapons' own shots: every `Shot` look a shipped unit names
//! as a weapon's travel, on a line half a unit above the ground, its `Critical` layers drawn
//! along the projected path (a beam from muzzle to impact, a slug's head or a projectile's body
//! where the shot has reached), nothing after its end, and nothing over the unseen half of its
//! path under the lab's fog ([`shipped_shots_draw_along_their_paths`]).
//!
//! # Invariants verified
//! As listed; every statistic is printed for the driver comparison.

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

#[allow(dead_code, reason = "shared with the catalog tests")]
#[path = "support/effect_subjects.rs"]
mod effect_subjects;

use bevy::math::{UVec2, Vec2, Vec3};
use bevy::render::camera::CameraProjection;
use client::effects::binding::resolve_value;
use client::effects::catalog::schema::{Binding, PlacementDef, Priority};
use client::effects::catalog::{Block, Effect, EffectCatalog, EFFECTS_CATALOG_PATH};
use client::effects::lab::program::{sequence_frames, sequence_take};
use client::effects::lab::{
    lab_app, LabGround, LabProgram, LabShot, LabSubject, LabTake, LabZoom, SEQUENCE_STEP,
};
use client::ResolvedAssetsRoot;
use effect_subjects::SplashSubject;
use image::RgbImage;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const OUT_VAR: &str = "SPACE_SCUFFLE_EFFECT_SLICE_OUT";
/// Environment variable carrying the child's subject index.
const INDEX_VAR: &str = "SPACE_SCUFFLE_EFFECT_SLICE_INDEX";
/// Environment variable that writes the golden stills instead of comparing them.
const UPDATE_VAR: &str = "SPACE_SCUFFLE_EFFECT_GOLDEN_UPDATE";
/// Environment variable keeping the captures in a directory.
const KEEP_VAR: &str = "SPACE_SCUFFLE_EFFECT_SLICE_KEEP";
/// Capture size (plan §10.2).
const SIZE: UVec2 = UVec2::new(1280, 720);
/// Footprint and line tolerance, pixels.
const TOLERANCE_PX: f32 = 2.0;
/// Share of a splash front's rays that must end within the tolerance.
const FRONT_RAYS_WITHIN: f32 = 0.85;
/// A pixel is changed by the effect when a channel differs from the empty ground by more.
const LIT_LEVELS: i32 = 16;
/// Coverage of the full-damage disc at peak.
const MIN_COVERAGE: f32 = 0.6;
/// Largest frame-to-frame change of the region's mean luminance after the impact, levels.
const MAX_POP_LEVELS: f32 = 6.0;
/// Largest channel difference from the empty ground after the effect's end.
const MAX_AFTER_LEVELS: i32 = 2;
/// Golden still ages, fractions of the lifetime.
const GOLDEN_AGES: [f32; 3] = [0.08, 0.3, 0.75];
/// Golden tolerance per pixel, levels.
const GOLDEN_LEVELS: i32 = 8;
/// Share of golden pixels that must be within the tolerance.
const GOLDEN_WITHIN: f32 = 0.99;
/// Largest mean absolute golden difference, levels.
const GOLDEN_MEAN: f32 = 1.5;
/// Margin after a layer's end before "after" captures, seconds.
const AFTER_MARGIN: f32 = 0.05;

// --- Geometry -----------------------------------------------------------------------------

/// The battle camera of the takes, its projection and the ground point under every pixel.
struct View {
    clip_from_world: bevy::math::Mat4,
    grounds: Vec<Option<Vec3>>,
}

impl View {
    fn new(focus: Vec2) -> Self {
        let projection = bevy::render::camera::PerspectiveProjection {
            aspect_ratio: SIZE.x as f32 / SIZE.y as f32,
            ..Default::default()
        };
        let camera = LabZoom::Default.camera(focus).compute_transform();
        let clip_from_world = projection.get_clip_from_view() * camera.compute_matrix().inverse();
        let world_from_clip = clip_from_world.inverse();
        let mut grounds = Vec::with_capacity((SIZE.x * SIZE.y) as usize);
        for y in 0..SIZE.y {
            for x in 0..SIZE.x {
                let ndc = Vec2::new(
                    (x as f32 + 0.5) / SIZE.x as f32 * 2.0 - 1.0,
                    1.0 - (y as f32 + 0.5) / SIZE.y as f32 * 2.0,
                );
                let near = world_from_clip.project_point3(ndc.extend(1.0));
                let far = world_from_clip.project_point3(ndc.extend(0.001));
                let direction = far - near;
                let hit = (direction.y.abs() >= 1e-6)
                    .then(|| -near.y / direction.y)
                    .filter(|t| *t > 0.0)
                    .map(|t| near + direction * t);
                grounds.push(hit);
            }
        }
        Self {
            clip_from_world,
            grounds,
        }
    }

    /// Pixel position of `point`.
    fn project(&self, point: Vec3) -> Vec2 {
        let ndc = self.clip_from_world.project_point3(point);
        Vec2::new(
            (ndc.x * 0.5 + 0.5) * SIZE.x as f32,
            (0.5 - ndc.y * 0.5) * SIZE.y as f32,
        )
    }

    /// Where the ray through the centre of pixel `(x, y)` meets the ground.
    fn ground(&self, x: u32, y: u32) -> Option<Vec3> {
        self.grounds[(y * SIZE.x + x) as usize]
    }
}

/// The lab subject of `subject`: at the origin, or its line along +x centred on the origin.
fn lab_subject(subject: &SplashSubject) -> LabSubject {
    let length = subject.bound.get(Binding::LineLength);
    let (anchor, line_to) = if subject.line {
        (
            Vec3::new(-length * 0.5, 0.0, 0.0),
            Some(Vec3::new(length * 0.5, 0.0, 0.0)),
        )
    } else {
        (Vec3::ZERO, None)
    };
    LabSubject {
        effect: subject.effect.clone(),
        bound: subject.bound,
        anchor,
        line_to,
        line_seen: u64::MAX,
        fog_below_x: None,
        repeat: None,
    }
}

// --- Timing from the vocabulary ---------------------------------------------------------------

/// The shipped vocabulary.
fn catalog() -> EffectCatalog {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads")
}

/// The longest delay `layer`'s line placements add on a line of `length`.
fn line_delay(layer: &client::effects::catalog::Layer, length: f32) -> f32 {
    layer
        .placements
        .iter()
        .map(|placement| match placement {
            PlacementDef::Line { travel, .. } if *travel > 0.0 => length / travel,
            _ => 0.0,
        })
        .fold(0.0, f32::max)
}

/// The effect's timing for `subject`: lifetime (the plan's duration), when every non-decal
/// layer has ended, when everything has ended, and the age of the footprint capture.
struct Timing {
    lifetime: f32,
    non_decal_end: f32,
    end: f32,
    footprint_age: f32,
}

/// The first life fraction at which `curve` reaches 1 (a ring's front at its reach).
fn reached(catalog: &EffectCatalog, curve: Option<client::effects::catalog::CurveId>) -> f32 {
    (0..=200)
        .map(|step| step as f32 / 200.0)
        .find(|life| catalog.sample(curve, *life, *life) >= 0.999)
        .unwrap_or(1.0)
}

fn timing(catalog: &EffectCatalog, effect: &Effect, subject: &SplashSubject) -> Timing {
    let length = subject.bound.get(Binding::LineLength);
    let ends = |decals: bool| {
        effect
            .layers
            .iter()
            .filter(|layer| decals || !matches!(layer.block, Block::Decal(_)))
            .map(|layer| layer.start + layer.duration + line_delay(layer, length))
            .fold(0.0, f32::max)
    };
    let critical = effect
        .layers
        .iter()
        .filter(|layer| layer.priority == Priority::Critical);
    let footprint_age = if subject.line {
        // The line's marks alone: after the last critical card has ended, while the marks hold.
        critical
            .filter(|layer| !matches!(layer.block, Block::Decal(_)))
            .map(|layer| layer.start + layer.duration + line_delay(layer, length))
            .fold(0.0, f32::max)
            + AFTER_MARGIN
    } else {
        // The front at its reach.
        critical
            .filter_map(|layer| match (&layer.block, layer.radius) {
                (Block::Ring { .. }, Some(radius)) => {
                    Some(layer.start + (reached(catalog, radius.curve) + 0.05) * layer.duration)
                }
                _ => None,
            })
            .fold(0.0, f32::max)
    };
    Timing {
        lifetime: effect.lifetime,
        non_decal_end: ends(false) + AFTER_MARGIN,
        end: ends(true) + AFTER_MARGIN,
        footprint_age,
    }
}

// --- Programs ---------------------------------------------------------------------------------

/// A take of single shots on the bright ground at the default zoom.
fn take(effect: bool, only: Option<Priority>, step: f32, shots: &[(&str, f32)]) -> LabTake {
    LabTake {
        ground: LabGround::Bright,
        zoom: LabZoom::Default,
        effect,
        only,
        step,
        shots: shots
            .iter()
            .map(|(name, age)| LabShot {
                age: *age,
                overlay: false,
                name: (*name).to_owned(),
            })
            .collect(),
    }
}

/// Child part `part` of subject `index`: 0 the stills, 1 and 2 the halves of the sequence.
fn program(index: usize, part: u32, out: PathBuf) -> LabProgram {
    let subjects = effect_subjects::splash_subjects();
    let subject = subjects.get(index).expect("the index names a subject");
    let catalog = catalog();
    let effect = catalog
        .effect(
            catalog
                .effect_id(&subject.effect)
                .expect("the effect exists"),
        )
        .expect("it resolves");
    let timing = timing(&catalog, effect, subject);
    let mut takes = vec![take(false, None, 0.25, &[("bg", 0.0)])];
    if part == 0 {
        takes.push(take(
            true,
            Some(Priority::Critical),
            SEQUENCE_STEP,
            &[
                ("critical", timing.footprint_age),
                ("critical_late", timing.non_decal_end),
            ],
        ));
        let mut stills: Vec<(String, f32)> = GOLDEN_AGES
            .iter()
            .enumerate()
            .map(|(index, fraction)| (format!("golden_{index}"), fraction * timing.lifetime))
            .collect();
        stills.push(("after_detail".into(), timing.non_decal_end));
        stills.push(("after_all".into(), timing.end));
        stills.sort_by(|a, b| a.1.total_cmp(&b.1));
        let shots: Vec<(&str, f32)> = stills
            .iter()
            .map(|(name, age)| (name.as_str(), *age))
            .collect();
        takes.push(take(true, None, SEQUENCE_STEP, &shots));
    } else {
        // The sequence in two halves, each a fresh spawn stepped through the same 40 ms steps,
        // so each child stays well inside the harness's run limit.
        let mut sequence = sequence_take(LabGround::Bright, LabZoom::Default, "seq");
        let half = sequence.shots.len() / 2;
        if part == 1 {
            sequence.shots.truncate(half);
        } else {
            sequence.shots.drain(..half);
        }
        takes.push(sequence);
    }
    LabProgram {
        subject: lab_subject(subject),
        takes,
        out,
    }
}

// --- Child ------------------------------------------------------------------------------------

/// The child: runs one part of one subject's program.
fn run_child(part: u32) {
    let out = PathBuf::from(std::env::var_os(OUT_VAR).expect("the parent names the directory"));
    let index: usize = std::env::var(INDEX_VAR)
        .expect("the parent names the subject")
        .parse()
        .expect("the index is a number");
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let mut app = lab_app(root, program(index, part, out), None, SIZE).expect("the lab app builds");
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// The stills child.
#[test]
#[ignore = "launched by the slice effect tests on a virtual display"]
fn effect_slice_stills_child() {
    run_child(0);
}

/// The first half of the sequence.
#[test]
#[ignore = "launched by the slice effect tests on a virtual display"]
fn effect_slice_sequence_child() {
    run_child(1);
}

/// The second half of the sequence.
#[test]
#[ignore = "launched by the slice effect tests on a virtual display"]
fn effect_slice_sequence_late_child() {
    run_child(2);
}

// --- Parent -----------------------------------------------------------------------------------

/// Reads capture `name`.
fn capture(directory: &Path, name: &str) -> RgbImage {
    image::open(directory.join(format!("{name}.png")))
        .unwrap_or_else(|error| panic!("capture {name}: {error}"))
        .to_rgb8()
}

/// Largest channel difference of pixel `(x, y)`.
fn difference(a: &RgbImage, b: &RgbImage, x: u32, y: u32) -> i32 {
    let (first, second) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
    (0..3)
        .map(|channel| (i32::from(first[channel]) - i32::from(second[channel])).abs())
        .max()
        .unwrap_or(0)
}

/// Whether pixel `(x, y)` is changed by the effect.
fn lit(image: &RgbImage, background: &RgbImage, x: u32, y: u32) -> bool {
    difference(image, background, x, y) > LIT_LEVELS
}

/// Rec. 709 luminance of a pixel, levels.
fn luminance(image: &RgbImage, x: u32, y: u32) -> f32 {
    let pixel = image.get_pixel(x, y).0;
    0.2126 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.0722 * f32::from(pixel[2])
}

/// The subject's ground geometry: anchor, line direction and length, zone radii.
struct Ground {
    anchor: Vec3,
    direction: Vec3,
    length: f32,
    full: f32,
    reach: f32,
}

impl Ground {
    fn of(subject: &LabSubject) -> Self {
        let to = subject.line_to.unwrap_or(subject.anchor);
        Self {
            anchor: subject.anchor,
            direction: (to - subject.anchor).normalize_or_zero(),
            length: subject.anchor.distance(to),
            full: subject.bound.get(Binding::SplashFull),
            reach: subject.bound.get(Binding::SplashReach),
        }
    }

    /// Distance of ground point `point` from the effect: from the anchor, or from the line.
    fn distance(&self, point: Vec3) -> f32 {
        let along = (point - self.anchor)
            .dot(self.direction)
            .clamp(0.0, self.length);
        point.distance(self.anchor + self.direction * along)
    }
}

/// Signed distance, in pixels, of the outermost lit pixel from the projected reach on each of
/// 72 rays from the anchor (NaN where a ray has no lit pixel).
fn reach_errors(view: &View, ground: &Ground, critical: &RgbImage, bg: &RgbImage) -> Vec<f32> {
    let rays = 72;
    let mut errors = Vec::with_capacity(rays);
    for ray in 0..rays {
        let angle = ray as f32 / rays as f32 * std::f32::consts::TAU;
        let direction = Vec3::new(angle.cos(), 0.0, angle.sin());
        let edge = view.project(ground.anchor + direction * ground.reach);
        let mut outer = None;
        let mut distance = ground.reach * 0.3;
        while distance <= ground.reach * 1.6 {
            let pixel = view.project(ground.anchor + direction * distance);
            let (x, y) = (pixel.x as u32, pixel.y as u32);
            if x < SIZE.x && y < SIZE.y && lit(critical, bg, x, y) {
                outer = Some(pixel);
            }
            distance += 0.004;
        }
        let Some(outer) = outer else {
            errors.push(f32::NAN);
            continue;
        };
        let centre = view.project(ground.anchor);
        errors.push(outer.distance(centre) - edge.distance(centre));
    }
    errors
}

/// Sectors of the circle in which a torn front's farthest clump must reach the reach.
const FRONT_SECTORS: usize = 8;

/// Whether a torn front's rays (`errors`, per ray, pixels; NaN where nothing is lit) show the
/// reach all the way round: in each of [`FRONT_SECTORS`] equal sectors the farthest ray ends
/// within [`TOLERANCE_PX`] of it. A front squashed on one side, or missing a quarter, fails.
fn torn_front_holds(errors: &[f32]) -> Result<(), String> {
    let per_sector = errors.len().div_ceil(FRONT_SECTORS).max(1);
    for (sector, rays) in errors.chunks(per_sector).enumerate() {
        let farthest = rays
            .iter()
            .copied()
            .filter(|error| error.is_finite())
            .fold(f32::NEG_INFINITY, f32::max);
        if farthest < -TOLERANCE_PX {
            return Err(format!(
                "sector {sector} of the torn front ends {farthest:.2} px inside the reach at most"
            ));
        }
    }
    Ok(())
}

/// The per-sector rule passes a torn front that reaches the reach all the way round, and
/// fails one squashed on one side or missing a quarter (Amendment 7).
#[test]
fn a_torn_front_must_reach_the_reach_in_every_sector() {
    // Clumps on the reach every third ray, bitten 5 px back between them.
    let torn: Vec<f32> = (0..72)
        .map(|ray| if ray % 3 == 0 { -1.0 } else { -5.0 })
        .collect();
    assert!(torn_front_holds(&torn).is_ok());
    // Squashed: one half of the circle ends 5 px short everywhere.
    let squashed: Vec<f32> = (0..72)
        .map(|ray| if ray < 36 { torn[ray] } else { -5.0 })
        .collect();
    assert!(torn_front_holds(&squashed).is_err());
    // A quarter missing: nothing lit over 18 rays.
    let quarter: Vec<f32> = (0..72)
        .map(|ray| {
            if (18..36).contains(&ray) {
                f32::NAN
            } else {
                torn[ray]
            }
        })
        .collect();
    assert!(torn_front_holds(&quarter).is_err());
}

/// The projected reach in pixels along each of `rays` rays from the anchor.
fn reach_pixels(view: &View, ground: &Ground, rays: usize) -> Vec<f32> {
    let centre = view.project(ground.anchor);
    (0..rays)
        .map(|ray| {
            let angle = ray as f32 / rays as f32 * std::f32::consts::TAU;
            let direction = Vec3::new(angle.cos(), 0.0, angle.sin());
            view.project(ground.anchor + direction * ground.reach)
                .distance(centre)
        })
        .collect()
}

/// The declared bite of `effect`'s torn `Critical` front, as a share of its reach: the
/// largest `edge_noise · thickness` of its critical rings (0 for round fronts).
fn front_bite(effect: &Effect) -> f32 {
    effect
        .layers
        .iter()
        .filter(|layer| layer.priority == Priority::Critical)
        .filter_map(|layer| match layer.block {
            Block::Ring {
                thickness,
                edge_noise,
                ..
            } => Some(thickness * edge_noise),
            _ => None,
        })
        .fold(0.0, f32::max)
}

/// Splash footprint: the front's outermost lit pixel per ray against the projected reach, and
/// the lasting marks (`late`, once every non-decal layer has ended) never beyond it.
fn check_front(
    view: &View,
    ground: &Ground,
    (critical, late, bg): (&RgbImage, &RgbImage, &RgbImage),
    bite: f32,
) -> String {
    let errors = reach_errors(view, ground, critical, bg);
    let rays = errors.len();
    // A torn front may end inside the reach by its declared bite (a share of the reach),
    // projected per ray; a round front (bite 0) keeps the plain tolerance.
    let reach_px = reach_pixels(view, ground, rays);
    let within = errors
        .iter()
        .zip(&reach_px)
        .filter(|(error, reach)| {
            **error <= TOLERANCE_PX && **error >= -(TOLERANCE_PX + bite * **reach)
        })
        .count();
    let worst_beyond = errors
        .iter()
        .copied()
        .filter(|error| error.is_finite())
        .fold(f32::MIN, f32::max);
    let late_errors = reach_errors(view, ground, late, bg);
    let late_beyond = late_errors
        .iter()
        .copied()
        .filter(|error| error.is_finite())
        .reduce(f32::max);
    // Review round 5: a lasting mark's visible edge lies on the reach, not only inside it.
    let late_within = late_errors
        .iter()
        .filter(|error| error.abs() <= TOLERANCE_PX)
        .count() as f32
        / rays as f32;
    let late_finite: Vec<f32> = late_errors
        .iter()
        .copied()
        .filter(|error| error.is_finite())
        .collect();
    let late_mean = late_finite.iter().sum::<f32>() / late_finite.len().max(1) as f32;
    assert!(
        late_finite.is_empty() || late_within >= FRONT_RAYS_WITHIN,
        "only {:.0} % of rays of the lasting critical marks end within {TOLERANCE_PX} px of the reach (errors {late_errors:?})",
        late_within * 100.0
    );
    let share = within as f32 / rays as f32;
    let finite: Vec<f32> = errors
        .iter()
        .copied()
        .filter(|error| error.is_finite())
        .collect();
    let mean = finite.iter().sum::<f32>() / finite.len().max(1) as f32;
    assert!(
        worst_beyond <= TOLERANCE_PX,
        "the front reaches {worst_beyond:.2} px beyond the reach"
    );
    assert!(
        late_beyond.is_none_or(|beyond| beyond <= TOLERANCE_PX),
        "the lasting critical marks reach {late_beyond:?} px beyond the reach"
    );
    assert!(
        share >= FRONT_RAYS_WITHIN,
        "only {:.0} % of rays end within {TOLERANCE_PX} px of the reach (bite {bite}; errors {errors:?})",
        share * 100.0
    );
    // The farthest-reaching clumps of a torn front still mark the reach all the way round
    // (Amendment 7: in every 45-degree sector, not only somewhere).
    if bite > 0.0 {
        if let Err(failure) = torn_front_holds(&errors) {
            panic!("{failure} (errors {errors:?})");
        }
    }
    format!(
        "footprint: {:.0} % of {rays} rays within ±{TOLERANCE_PX} px of the reach{}, mean error {mean:+.2} px, largest overshoot {worst_beyond:+.2} px; lasting marks' visible edge {}",
        share * 100.0,
        if bite > 0.0 {
            format!(" (or inside it by up to its declared bite, {:.0} % of the reach)", bite * 100.0)
        } else {
            String::new()
        },
        late_beyond.map_or_else(
            || "none (no lasting critical mark)".to_owned(),
            |beyond| format!(
                "{:.0} % of rays within ±{TOLERANCE_PX} px, mean error {late_mean:+.2} px, largest overshoot {beyond:+.2} px",
                late_within * 100.0
            )
        )
    )
}

/// Line footprint: the lit marks' extent along the line against its projected ends.
fn check_line(view: &View, ground: &Ground, critical: &RgbImage, bg: &RgbImage) -> String {
    let (mut first, mut last) = (f32::MAX, f32::MIN);
    for y in 0..SIZE.y {
        for x in 0..SIZE.x {
            if !lit(critical, bg, x, y) {
                continue;
            }
            let Some(point) = view.ground(x, y) else {
                continue;
            };
            let along = (point - ground.anchor).dot(ground.direction);
            first = first.min(along);
            last = last.max(along);
        }
    }
    assert!(first < last, "the line's marks lit nothing");
    let px = |along: f32| view.project(ground.anchor + ground.direction * along);
    let start_error = px(first).distance(px(0.0)) * (first - 0.0).signum();
    let end_error = px(last).distance(px(ground.length)) * (last - ground.length).signum();
    assert!(
        start_error.abs() <= TOLERANCE_PX && end_error.abs() <= TOLERANCE_PX,
        "the marks run from {first:.3} to {last:.3} (line 0 to {:.3}): start {start_error:+.2} px, end {end_error:+.2} px",
        ground.length
    );
    format!(
        "footprint: marks start {start_error:+.2} px and end {end_error:+.2} px from the line's projected ends"
    )
}

/// Points whose full-damage discs the coverage is measured on, and the frame each peaks by.
fn coverage_points(ground: &Ground, subject: &LabSubject, travel: f32) -> Vec<(Vec3, f32)> {
    if subject.line_to.is_none() {
        return vec![(ground.anchor, 0.0)];
    }
    let spacing = subject.bound.get(Binding::LineSpacing).max(1e-3);
    let count = (ground.length / spacing).round() as u32 + 1;
    (0..count)
        .map(|index| {
            let along = index as f32 * spacing;
            let delay = if travel > 0.0 { along / travel } else { 0.0 };
            (ground.anchor + ground.direction * along, delay)
        })
        .collect()
}

/// Coverage of each full-damage disc within 80 ms of its event; returns the mean.
fn check_coverage(
    view: &View,
    ground: &Ground,
    points: &[(Vec3, f32)],
    frames: &[RgbImage],
    bg: &RgbImage,
) -> String {
    let mut coverages = Vec::with_capacity(points.len());
    for (centre, delay) in points {
        let first = (delay / SEQUENCE_STEP).floor() as usize;
        let mut best = 0.0_f32;
        for frame in frames.iter().skip(first).take(3) {
            let (mut inside, mut changed) = (0_u32, 0_u32);
            for y in 0..SIZE.y {
                for x in 0..SIZE.x {
                    let Some(point) = view.ground(x, y) else {
                        continue;
                    };
                    if point.distance(*centre) > ground.full {
                        continue;
                    }
                    inside += 1;
                    if lit(frame, bg, x, y) {
                        changed += 1;
                    }
                }
            }
            best = best.max(changed as f32 / inside.max(1) as f32);
        }
        coverages.push(best);
    }
    let mean = coverages.iter().sum::<f32>() / coverages.len().max(1) as f32;
    let least = coverages.iter().copied().fold(1.0, f32::min);
    assert!(
        mean >= MIN_COVERAGE,
        "peak coverage of the full-damage disc {:.0} % (per point {coverages:?})",
        mean * 100.0
    );
    format!(
        "coverage: {:.0} % of the full-damage disc at peak (least point {:.0} %)",
        mean * 100.0,
        least * 100.0
    )
}

/// The effect's region: pixels whose ground point is within `margin` of the effect.
fn region(view: &View, ground: &Ground, margin: f32) -> Vec<(u32, u32)> {
    let mut pixels = Vec::new();
    for y in 0..SIZE.y {
        for x in 0..SIZE.x {
            if view
                .ground(x, y)
                .is_some_and(|point| ground.distance(point) <= margin)
            {
                pixels.push((x, y));
            }
        }
    }
    pixels
}

/// No pop: consecutive frames after the impact frame change the region's mean luminance by
/// at most [`MAX_POP_LEVELS`].
fn check_pop(region: &[(u32, u32)], frames: &[RgbImage]) -> String {
    let means: Vec<f32> = frames
        .iter()
        .map(|frame| {
            region
                .iter()
                .map(|(x, y)| luminance(frame, *x, *y))
                .sum::<f32>()
                / region.len().max(1) as f32
        })
        .collect();
    let (mut worst, mut at) = (0.0_f32, 0);
    for index in 1..means.len().saturating_sub(1) {
        let change = (means[index + 1] - means[index]).abs();
        if change > worst {
            worst = change;
            at = index;
        }
    }
    let impact = (means.get(1).copied().unwrap_or(0.0) - means[0]).abs();
    assert!(
        worst <= MAX_POP_LEVELS,
        "a pop of {worst:.2} levels between frames {at} and {} (means {means:?})",
        at + 1
    );
    format!(
        "pop: largest change after the impact {worst:.2}/255 (frames {at}→{}), impact frame {impact:.2}/255, over {} px",
        at + 1,
        region.len()
    )
}

/// Largest extent of the effect's decals: their size times jitter, on a splash or a line, out
/// to the corner of the projection box (half extents the size and the size times the width,
/// turned to any yaw), since a decal's texture may reach into the box's corners.
fn decal_margin(effect: &Effect, subject: &LabSubject, catalog: &EffectCatalog) -> f32 {
    effect
        .layers
        .iter()
        .filter_map(|layer| match &layer.block {
            Block::Decal(look) => Some((layer, look.width)),
            _ => None,
        })
        .map(|(layer, width)| {
            let grow = (0..=20)
                .map(|step| catalog.sample(layer.size_curve, step as f32 / 20.0, 1.0))
                .fold(0.0, f32::max);
            let offset = layer
                .placements
                .iter()
                .map(|placement| match placement {
                    PlacementDef::Disc { radius, .. } | PlacementDef::Circle { radius, .. } => {
                        resolve_value(radius, &subject.bound)
                    }
                    _ => 0.0,
                })
                .fold(0.0, f32::max);
            let corner = (1.0 + width * width).sqrt();
            offset
                + resolve_value(&layer.size, &subject.bound)
                    * (1.0 + layer.size_jitter)
                    * grow
                    * corner
        })
        .fold(0.0, f32::max)
}

/// Nothing lingers: after the detail only the decals' footprint differs; after the end
/// nothing does.
fn check_after(
    view: &View,
    ground: &Ground,
    margin: f32,
    directory: &Path,
    bg: &RgbImage,
) -> String {
    let detail = capture(directory, "after_detail");
    let all = capture(directory, "after_all");
    let (mut stray, mut decal) = (0_u32, 0_u32);
    let mut after = 0;
    for y in 0..SIZE.y {
        for x in 0..SIZE.x {
            after = after.max(difference(&all, bg, x, y));
            if !lit(&detail, bg, x, y) {
                continue;
            }
            let inside = view
                .ground(x, y)
                .is_some_and(|point| ground.distance(point) <= margin + 0.05);
            if inside {
                decal += 1;
            } else {
                stray += 1;
            }
        }
    }
    assert_eq!(
        stray, 0,
        "{stray} px lit outside the decals once the detail ended"
    );
    assert!(
        after <= MAX_AFTER_LEVELS,
        "{after} levels from the empty ground after the effect's end"
    );
    format!(
        "lingering: {decal} decal px and {stray} other px after the detail, largest difference after the end {after}/255"
    )
}

/// The golden region: the bounding box of the effect's region, padded upwards for cards.
fn golden_box(region: &[(u32, u32)]) -> (u32, u32, u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y) in region {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let y0 = y0.saturating_sub((y1 - y0) / 2);
    (x0, y0, x1 - x0 + 1, y1 - y0 + 1)
}

/// Golden stills of this driver.
fn check_goldens(
    directory: &Path,
    effect: &str,
    driver: &str,
    region: &[(u32, u32)],
) -> TestResult<String> {
    let (x, y, width, height) = golden_box(region);
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/effects")
        .join(driver);
    let update = std::env::var_os(UPDATE_VAR).is_some();
    let mut report = Vec::with_capacity(GOLDEN_AGES.len());
    for index in 0..GOLDEN_AGES.len() {
        let still = image::imageops::crop_imm(
            &capture(directory, &format!("golden_{index}")),
            x,
            y,
            width,
            height,
        )
        .to_image();
        let path = goldens.join(format!("{effect}_{index}.png"));
        if update {
            std::fs::create_dir_all(&goldens)?;
            still.save(&path)?;
            report.push(format!("{} written", path.display()));
            continue;
        }
        let golden = image::open(&path)
            .unwrap_or_else(|error| {
                panic!(
                    "golden {}: {error} (write it with {UPDATE_VAR}=1)",
                    path.display()
                )
            })
            .to_rgb8();
        assert_eq!(
            golden.dimensions(),
            still.dimensions(),
            "golden {} size",
            path.display()
        );
        let (mut within, mut total) = (0_u32, 0_u64);
        for (a, b) in golden.pixels().zip(still.pixels()) {
            let largest = (0..3)
                .map(|channel| (i32::from(a.0[channel]) - i32::from(b.0[channel])).abs())
                .max()
                .unwrap_or(0);
            within += u32::from(largest <= GOLDEN_LEVELS);
            total += (0..3)
                .map(|channel| u64::from(a.0[channel].abs_diff(b.0[channel])))
                .sum::<u64>();
        }
        let pixels = (width * height) as f32;
        let share = within as f32 / pixels;
        let mean = total as f32 / (pixels * 3.0);
        assert!(
            share >= GOLDEN_WITHIN && mean <= GOLDEN_MEAN,
            "golden {}: {:.2} % within {GOLDEN_LEVELS}/255, mean {mean:.3}/255",
            path.display(),
            share * 100.0
        );
        report.push(format!(
            "golden {index}: {:.2} % within, mean {mean:.3}/255",
            share * 100.0
        ));
    }
    Ok(report.join("; "))
}

/// Runs both children of subject `index` and checks every statistic.
fn check_subject(index: usize) -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let subjects = effect_subjects::splash_subjects();
    let Some(subject) = subjects.get(index) else {
        return Ok(());
    };
    let directory = tempfile::tempdir()?;
    let mut driver = None;
    for (child, label) in [
        ("effect_slice_stills_child", "slice stills"),
        ("effect_slice_sequence_child", "slice sequence"),
        (
            "effect_slice_sequence_late_child",
            "slice sequence, second half",
        ),
    ] {
        let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
        command.args(["--ignored", "--exact", "--nocapture", child]);
        command.env(OUT_VAR, directory.path());
        command.env(INDEX_VAR, index.to_string());
        driver = command.driver();
        command.assert_passes_validation(label);
    }
    let driver = driver.map_or("unknown", gpu_run::GpuDriver::name);
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        let keep = PathBuf::from(keep).join(driver).join(&subject.effect);
        std::fs::create_dir_all(&keep)?;
        for entry in std::fs::read_dir(directory.path())? {
            let entry = entry?;
            std::fs::copy(entry.path(), keep.join(entry.file_name()))?;
        }
    }
    let lab = lab_subject(subject);
    let view = View::new(lab.focus());
    let ground = Ground::of(&lab);
    let catalog = catalog();
    let effect = catalog
        .effect(
            catalog
                .effect_id(&subject.effect)
                .expect("the effect exists"),
        )
        .expect("it resolves");
    let bg = capture(directory.path(), "bg");
    let critical = capture(directory.path(), "critical");
    let footprint = if lab.line_to.is_some() {
        check_line(&view, &ground, &critical, &bg)
    } else {
        let late = capture(directory.path(), "critical_late");
        check_front(&view, &ground, (&critical, &late, &bg), front_bite(effect))
    };
    let frames: Vec<RgbImage> = (0..sequence_frames())
        .map(|frame| capture(directory.path(), &format!("seq_{frame:03}")))
        .collect();
    let travel = effect
        .layers
        .iter()
        .flat_map(|layer| layer.placements.iter())
        .find_map(|placement| match placement {
            PlacementDef::Line { travel, .. } => Some(*travel),
            _ => None,
        })
        .unwrap_or(0.0);
    let coverage = check_coverage(
        &view,
        &ground,
        &coverage_points(&ground, &lab, travel),
        &frames,
        &bg,
    );
    let margin = ground.reach.max(1.0) * 3.0;
    let effect_region = region(&view, &ground, margin);
    let pop = check_pop(&effect_region, &frames);
    let after = check_after(
        &view,
        &ground,
        decal_margin(effect, &lab, &catalog),
        directory.path(),
        &bg,
    );
    // The golden stills keep the effect's own neighbourhood, so the files stay small.
    let golden_region = region(&view, &ground, ground.reach.max(1.0) * 1.5);
    let goldens = check_goldens(directory.path(), &subject.effect, driver, &golden_region)?;
    eprintln!(
        "slice effect {} ({}) on {driver}:\n  {footprint}\n  {coverage}\n  {pop}\n  {after}\n  {goldens}",
        subject.effect, subject.unit
    );
    Ok(())
}

/// The first splash subject (by name).
#[test]
fn slice_effect_0_renders_to_its_bindings() -> TestResult {
    check_subject(0)
}

/// The second splash subject (by name).
#[test]
fn slice_effect_1_renders_to_its_bindings() -> TestResult {
    check_subject(1)
}

/// The third splash subject (by name).
#[test]
fn slice_effect_2_renders_to_its_bindings() -> TestResult {
    check_subject(2)
}

/// The fourth splash subject (by name; the scale-out's effects from WP7 on).
#[test]
fn slice_effect_3_renders_to_its_bindings() -> TestResult {
    check_subject(3)
}

/// The fifth splash subject (by name).
#[test]
fn slice_effect_4_renders_to_its_bindings() -> TestResult {
    check_subject(4)
}

/// The sixth splash subject (by name).
#[test]
fn slice_effect_5_renders_to_its_bindings() -> TestResult {
    check_subject(5)
}

/// The seventh splash subject (by name).
#[test]
fn slice_effect_6_renders_to_its_bindings() -> TestResult {
    check_subject(6)
}

/// No splash subject is left without a test of its own above.
#[test]
fn every_splash_subject_has_its_own_test() {
    assert!(
        effect_subjects::splash_subjects().len() <= 7,
        "add a slice_effect_N test for each subject beyond the seventh"
    );
}

// --- A line out of the fog (plan Amendment 2, R6) ---------------------------------------------

/// The first search point of the masked line that the viewer sees.
const FIRST_SEEN_POINT: u32 = 4;
/// Age of the masked shot, seconds: every spine of the line is up.
const MASKED_AGE: f32 = 1.0;
/// How far, in world units along the line, a drawn instance at the first seen point may reach
/// back towards the unseen points: ground marks and dust are clipped half a spacing before the
/// first seen point (review round 4, their end feather reaching 0.18 beyond), and the clump
/// standing on that point is kept whole, its splayed spikes reaching up to about 0.9 back.
const MASKED_REACH_BACK: f32 = 1.0;
/// Fewest lit pixels the seen part of the line must draw.
const MASKED_MIN_LIT: usize = 200;
/// The indices of the line subjects.
fn line_subjects() -> Vec<usize> {
    effect_subjects::splash_subjects()
        .iter()
        .enumerate()
        .filter(|(_, subject)| subject.line)
        .map(|(index, _)| index)
        .collect()
}

/// The masked child: the line subject named by [`INDEX_VAR`] seen only from point
/// [`FIRST_SEEN_POINT`] on, and the background.
#[test]
#[ignore = "launched by the slice effect tests on a virtual display"]
fn effect_slice_masked_line_child() {
    let out = PathBuf::from(std::env::var_os(OUT_VAR).expect("the parent names the directory"));
    let index: usize = std::env::var(INDEX_VAR)
        .expect("the parent names the subject")
        .parse()
        .expect("the index is a number");
    let subjects = effect_subjects::splash_subjects();
    let subject = &subjects[index];
    assert!(subject.line, "the masked child draws a line subject");
    let mut lab = lab_subject(subject);
    lab.line_seen = u64::MAX << FIRST_SEEN_POINT;
    let program = LabProgram {
        subject: lab,
        takes: vec![
            take(false, None, 0.25, &[("bg", 0.0)]),
            take(true, None, SEQUENCE_STEP, &[("masked", MASKED_AGE)]),
        ],
        out,
    };
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let mut app = lab_app(root, program, None, SIZE).expect("the lab app builds");
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// Plan Amendment 2, R6, in pixels: a line whose first search points the viewer does not see
/// (an undetected burrowed attacker firing out of the fog) draws nothing at those points and
/// its whole look from the first seen point on; every line subject is checked.
#[test]
fn a_line_seen_only_in_part_draws_only_where_it_is_seen() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    for index in line_subjects() {
        check_masked_line(index)?;
    }
    Ok(())
}

/// Runs the masked child for line subject `index` and checks its pixels.
fn check_masked_line(index: usize) -> TestResult {
    let subjects = effect_subjects::splash_subjects();
    let subject = &subjects[index];
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args([
        "--ignored",
        "--exact",
        "--nocapture",
        "effect_slice_masked_line_child",
    ]);
    command.env(OUT_VAR, directory.path());
    command.env(INDEX_VAR, index.to_string());
    command.assert_passes_validation("slice masked line");
    let lab = lab_subject(subject);
    let view = View::new(lab.focus());
    let ground = Ground::of(&lab);
    let spacing = subject.bound.get(Binding::LineSpacing);
    #[expect(clippy::cast_precision_loss, reason = "a small point index")]
    let first_seen = FIRST_SEEN_POINT as f32 * spacing;
    let bg = capture(directory.path(), "bg");
    let masked = capture(directory.path(), "masked");
    let (mut unseen_lit, mut seen_lit) = (0_usize, 0_usize);
    for y in 0..SIZE.y {
        for x in 0..SIZE.x {
            if !lit(&masked, &bg, x, y) {
                continue;
            }
            let Some(point) = view.ground(x, y) else {
                continue;
            };
            let along = (point - ground.anchor).dot(ground.direction);
            if along < first_seen - MASKED_REACH_BACK {
                unseen_lit += 1;
            } else if along >= first_seen {
                seen_lit += 1;
            }
        }
    }
    eprintln!(
        "masked line {}: {seen_lit} lit pixels from point {FIRST_SEEN_POINT} on, {unseen_lit} \
         before it (more than {MASKED_REACH_BACK} back)",
        subject.effect
    );
    assert_eq!(
        unseen_lit, 0,
        "{}: nothing is drawn at the search points the viewer does not see",
        subject.effect
    );
    assert!(
        seen_lit >= MASKED_MIN_LIT,
        "{}: the seen part of the line is drawn: {seen_lit} lit pixels",
        subject.effect
    );
    Ok(())
}

// --- WP11: the weapons' own shots ---------------------------------------------------------------
//
// Plan WP11 (Amendment 24): every `Shot` effect a shipped unit names for a weapon's `travel`,
// in the production lab on a line of `SHOT_LENGTH` along +x half a unit above the bright ground
// (where the simulation publishes shots), with the flight a projectile's weapon binds. The
// parent checks, on the `Critical` layers alone, that the shot draws along its path:
// - every changed pixel lies within its projected half width (plus [`TOLERANCE_PX`]) of the
//   projected path;
// - a strip spanning the whole line (a beam) ends within [`SHOT_END_TOLERANCE_PX`] of the
//   projected muzzle and impact;
// - a strip riding the line (a tracer's slug, a projectile's trail) has its head within
//   [`SHOT_END_TOLERANCE_PX`] of the point the shot's life has reached, and a riding card (a
//   projectile's body) is centred on that point within [`SHOT_END_TOLERANCE_PX`];
// - nothing of the shot is drawn once its layers have ended;
// - under the lab's fog over x < 0 (the muzzle in the fog, the impact seen), no pixel over the
//   unseen half changes, at an age when its body is still in the fog and at one when it is
//   seen.

/// Environment variable carrying the shot child's part: 0 the clear stills, 1 the fog stills.
const SHOT_PART_VAR: &str = "SPACE_SCUFFLE_EFFECT_SHOT_PART";
/// Length of the shot subjects' line, world units.
const SHOT_LENGTH: f32 = 6.0;
/// Flight a projectile subject binds, seconds.
const SHOT_FLIGHT: f32 = 0.6;
/// Share of a shot's life at which its path is measured.
const SHOT_LIFE: f32 = 0.5;
/// Tolerance of a shot's ends and of its riding body's place, pixels: [`TOLERANCE_PX`] plus
/// one pixel for the soft rounded tips of the Gaussian strips, whose last pixel fades below the
/// lit threshold.
const SHOT_END_TOLERANCE_PX: f32 = 3.0;
/// Height of the shot subjects' line above the ground (`client::effects::lab::driver`).
const SHOT_LIFT: f32 = client::effects::lab::driver::SHOT_HEIGHT;

/// A shipped shot effect and the flight its weapons bind.
#[derive(Debug, Clone)]
struct ShotSubject {
    effect: String,
    flight: f32,
}

/// Every `Shot` effect a shipped unit's block names as a weapon's travel, sorted by name.
fn shot_subjects() -> Vec<ShotSubject> {
    let catalog = catalog();
    let mut names: Vec<String> = sim_fixtures::shipped_catalogs()
        .units
        .archetypes()
        .flat_map(|archetype| archetype.effects.weapons.clone())
        .filter_map(|entry| entry.travel)
        .collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|effect| {
            let resolved = catalog
                .effect(catalog.effect_id(&effect).expect("defined"))
                .expect("resolves");
            let flies = resolved
                .layers
                .iter()
                .any(|layer| layer.lasts == Some(Binding::FlightTime));
            ShotSubject {
                effect,
                flight: if flies { SHOT_FLIGHT } else { 0.0 },
            }
        })
        .collect()
}

/// The lab subject of `shot`: its line along +x centred on the origin, under the lab's fog over
/// x < 0 when `fog`.
fn shot_lab(shot: &ShotSubject, fog: bool) -> LabSubject {
    let bound = client::effects::binding::BoundValues::default()
        .with(Binding::LineLength, SHOT_LENGTH)
        .with(Binding::FlightTime, shot.flight);
    LabSubject {
        effect: shot.effect.clone(),
        bound,
        anchor: Vec3::new(-SHOT_LENGTH * 0.5, 0.0, 0.0),
        line_to: Some(Vec3::new(SHOT_LENGTH * 0.5, 0.0, 0.0)),
        line_seen: u64::MAX,
        fog_below_x: fog.then_some(0.0),
        repeat: None,
    }
}

/// The beside-the-fog take of `shot` (review 1, E1-2 (d)): its line along the camera's depth
/// axis (+z), `beside` world units on the seen side of the lab's fog edge at x = 0, so the
/// fog lies beside the whole path, within its reach.
fn shot_beside_fog(shot: &ShotSubject, beside: f32) -> LabSubject {
    LabSubject {
        anchor: Vec3::new(beside, 0.0, -SHOT_LENGTH * 0.5),
        line_to: Some(Vec3::new(beside, 0.0, SHOT_LENGTH * 0.5)),
        fog_below_x: Some(0.0),
        ..shot_lab(shot, false)
    }
}

/// How far beside the fog edge the beside-the-fog takes run: within the reach of the shot's
/// flying layers (`far` false: the shot is cut whole, fail-closed), or beyond the reach of every
/// layer of it (`far` true: the positive control, drawn whole; review 2, E2-1).
fn beside_offset(effect: &Effect, far: bool) -> f32 {
    let reach = effect.shot_reach;
    if far {
        1.3 * reach.muzzle.max(reach.path).max(0.35)
    } else {
        0.6 * reach.path.max(0.35)
    }
}

/// The shot's life in seconds: its longest non-held layer's span for its flight.
fn shot_lifetime(effect: &Effect, shot: &ShotSubject) -> f32 {
    let bound =
        client::effects::binding::BoundValues::default().with(Binding::FlightTime, shot.flight);
    client::effects::runtime::one_shot_lifetime(effect, &bound)
}

/// The age at which `shot`'s path is measured: [`SHOT_LIFE`] of its `Critical` layers' life.
fn shot_age(effect: &Effect, shot: &ShotSubject) -> f32 {
    let bound =
        client::effects::binding::BoundValues::default().with(Binding::FlightTime, shot.flight);
    effect
        .layers
        .iter()
        .filter(|layer| layer.priority == Priority::Critical)
        .map(|layer| layer.start + layer.span(&bound) * SHOT_LIFE)
        .fold(0.0, f32::max)
}

/// The shot child: part 0 the background, the `Critical` layers at the measured age and the
/// whole look after its end; part 1 the background under the fog and the whole look in it at a
/// quarter and at three quarters of its `Critical` layers' life (the body in the fog, then
/// seen).
#[test]
#[ignore = "launched by the shot effect test on a virtual display"]
fn effect_shot_child() {
    let out = PathBuf::from(std::env::var_os(OUT_VAR).expect("the parent names the directory"));
    let index: usize = std::env::var(INDEX_VAR)
        .expect("the parent names the subject")
        .parse()
        .expect("the index is a number");
    let part = std::env::var(SHOT_PART_VAR).unwrap_or_default();
    let fog = part == "1";
    let beside = part == "2" || part == "3";
    let far = part == "3";
    let prefix = if far { "far" } else { "side" };
    let subjects = shot_subjects();
    let shot = &subjects[index];
    let catalog = catalog();
    let effect = catalog
        .effect(catalog.effect_id(&shot.effect).expect("defined"))
        .expect("resolves");
    let age = shot_age(effect, shot);
    let lifetime = shot_lifetime(effect, shot);
    let takes = if beside {
        vec![
            take(false, None, 0.25, &[(&format!("{prefix}_bg"), 0.0)]),
            take(
                true,
                None,
                0.02,
                &[
                    (&format!("{prefix}_early"), age / SHOT_LIFE * 0.25),
                    (&format!("{prefix}_mid"), age / SHOT_LIFE * 0.5),
                    (&format!("{prefix}_late"), age / SHOT_LIFE * 0.75),
                ],
            ),
        ]
    } else if fog {
        vec![
            take(false, None, 0.25, &[("fog_bg", 0.0)]),
            take(
                true,
                None,
                0.02,
                &[
                    ("fog_early", age / SHOT_LIFE * 0.25),
                    ("fog_late", age / SHOT_LIFE * 0.75),
                ],
            ),
        ]
    } else {
        vec![
            take(false, None, 0.25, &[("bg", 0.0)]),
            take(true, Some(Priority::Critical), 0.02, &[("critical", age)]),
            take(true, None, 0.05, &[("after", lifetime + AFTER_MARGIN)]),
        ]
    };
    let program = LabProgram {
        subject: if beside {
            shot_beside_fog(shot, beside_offset(effect, far))
        } else {
            shot_lab(shot, fog)
        },
        takes,
        out,
    };
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let mut app = lab_app(root, program, None, SIZE).expect("the lab app builds");
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// Where along the projected path from `from` to `to` (pixels) pixel `pixel` lies, as a
/// distance from `from` in pixels, and how far it lies off the path.
fn along_and_across(from: Vec2, to: Vec2, pixel: Vec2) -> (f32, f32) {
    let axis = (to - from).normalize_or_zero();
    let offset = pixel - from;
    let along = offset.dot(axis);
    (along, (offset - axis * along).length())
}

/// The widest projected half width, pixels, of `effect`'s `Critical` layers on the path.
fn critical_half_width(effect: &Effect, view: &View, from: Vec3, to: Vec3) -> f32 {
    let mid = (from + to) * 0.5;
    effect
        .layers
        .iter()
        .filter(|layer| layer.priority == Priority::Critical)
        .map(|layer| {
            // A card drawn out along the path keeps its half size across it; a disc placement
            // moves it off the path by up to its radius, a pair by its offset.
            let scattered = layer
                .placements
                .iter()
                .map(|placement| match placement {
                    PlacementDef::Disc { radius, .. } => radius.scale,
                    PlacementDef::Across { offset } => offset.scale,
                    _ => 0.0,
                })
                .fold(0.0, f32::max);
            let across = layer.size.scale + scattered;
            let offset = view.project(mid + Vec3::Z * across);
            offset.distance(view.project(mid))
        })
        .fold(0.0, f32::max)
}

/// Checks the `Critical` layers of shot subject `index` along its path and under the fog.
fn check_shot(index: usize) -> TestResult<String> {
    let subjects = shot_subjects();
    let shot = &subjects[index];
    let directory = tempfile::tempdir()?;
    for (part, label) in [
        ("0", "shot stills"),
        ("1", "shot under fog"),
        ("2", "shot beside the fog"),
        ("3", "shot beyond its reach of the fog"),
    ] {
        let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
        command.args(["--ignored", "--exact", "--nocapture", "effect_shot_child"]);
        command.env(OUT_VAR, directory.path());
        command.env(INDEX_VAR, index.to_string());
        command.env(SHOT_PART_VAR, part);
        command.assert_passes_validation(label);
    }
    let catalog = catalog();
    let effect = catalog
        .effect(catalog.effect_id(&shot.effect).expect("defined"))
        .expect("resolves");
    let lab = shot_lab(shot, false);
    let view = View::new(lab.focus());
    let from = Vec3::new(-SHOT_LENGTH * 0.5, SHOT_LIFT, 0.0);
    let to = Vec3::new(SHOT_LENGTH * 0.5, SHOT_LIFT, 0.0);
    let (from_px, to_px) = (view.project(from), view.project(to));
    let length_px = from_px.distance(to_px);
    let half_width = critical_half_width(effect, &view, from, to);
    let bg = capture(directory.path(), "bg");
    let critical = capture(directory.path(), "critical");
    let (mut first, mut last, mut widest) = (f32::MAX, f32::MIN, 0.0_f32);
    let (mut sum, mut count) = (0.0_f32, 0_u32);
    for y in 0..SIZE.y {
        for x in 0..SIZE.x {
            if !lit(&critical, &bg, x, y) {
                continue;
            }
            let (along, across) =
                along_and_across(from_px, to_px, Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
            first = first.min(along);
            last = last.max(along);
            widest = widest.max(across);
            sum += along;
            count += 1;
        }
    }
    assert!(
        count > 0,
        "{}: the Critical layers draw nothing",
        shot.effect
    );
    assert!(
        widest <= half_width + TOLERANCE_PX,
        "{}: a pixel {widest:.1} px off the path (half width {half_width:.1} px)",
        shot.effect
    );
    let rides = effect.layers.iter().any(|layer| {
        layer.priority == Priority::Critical
            && match &layer.block {
                Block::Sprite(look) => look.ride,
                _ => false,
            }
    });
    let trails = effect.layers.iter().any(|layer| {
        layer.priority == Priority::Critical
            && matches!(&layer.block, Block::Ribbon(look) if look.trail.is_some())
    });
    let reached = length_px * SHOT_LIFE;
    let path = if rides {
        let centre = sum / count as f32;
        assert!(
            (centre - reached).abs() <= SHOT_END_TOLERANCE_PX,
            "{}: the body is centred {centre:.1} px along, the shot reached {reached:.1} px",
            shot.effect
        );
        format!(
            "body centred {:+.2} px from where the flight reached",
            centre - reached
        )
    } else if trails {
        assert!(
            (last - reached).abs() <= SHOT_END_TOLERANCE_PX,
            "{}: the slug's head is {last:.1} px along, the shot reached {reached:.1} px",
            shot.effect
        );
        format!(
            "slug head {:+.2} px from where the shot reached",
            last - reached
        )
    } else {
        assert!(
            first.abs() <= SHOT_END_TOLERANCE_PX
                && (last - length_px).abs() <= SHOT_END_TOLERANCE_PX,
            "{}: the beam runs {first:.1} to {last:.1} px of a {length_px:.1} px path",
            shot.effect
        );
        format!(
            "beam ends {:+.2} px and {:+.2} px from the muzzle and the impact",
            first,
            last - length_px
        )
    };
    let after = capture(directory.path(), "after");
    let lingering = (0..SIZE.y)
        .flat_map(|y| (0..SIZE.x).map(move |x| (x, y)))
        .filter(|(x, y)| difference(&after, &bg, *x, *y) > MAX_AFTER_LEVELS)
        .count();
    assert_eq!(
        lingering, 0,
        "{}: nothing is drawn once it has ended",
        shot.effect
    );
    let fog_bg = capture(directory.path(), "fog_bg");
    // The fog's edge (x = 0) on the projected path: everything drawn lies on the path, so a
    // pixel short of this point along it is drawn over the unseen half.
    let (edge_px, _) =
        along_and_across(from_px, to_px, view.project(Vec3::new(0.0, SHOT_LIFT, 0.0)));
    let mut seen_lit = 0;
    for name in ["fog_early", "fog_late"] {
        let fogged = capture(directory.path(), name);
        let mut unseen_lit = 0;
        for y in 0..SIZE.y {
            for x in 0..SIZE.x {
                if !lit(&fogged, &fog_bg, x, y) {
                    continue;
                }
                let (along, _) =
                    along_and_across(from_px, to_px, Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                if along < edge_px - TOLERANCE_PX {
                    unseen_lit += 1;
                } else {
                    seen_lit += 1;
                }
            }
        }
        assert_eq!(
            unseen_lit, 0,
            "{} {name}: nothing over the unseen half of the path",
            shot.effect
        );
    }
    assert!(
        seen_lit > 0,
        "{}: the seen half of the shot is drawn",
        shot.effect
    );
    // E1-2 (d): the fog beside the path, along the camera's depth axis. No pixel whose ray
    // meets the ground over the unseen side (x < 0, less half a pixel's worth of ground) may
    // change.
    let beside_take = |far: bool| -> (usize, usize) {
        let prefix = if far { "far" } else { "side" };
        let view = View::new(shot_beside_fog(shot, beside_offset(effect, far)).focus());
        let bg = capture(directory.path(), &format!("{prefix}_bg"));
        let (mut drawn, mut unseen) = (0, 0);
        for name in ["early", "mid", "late"] {
            let side = capture(directory.path(), &format!("{prefix}_{name}"));
            for y in 0..SIZE.y {
                for x in 0..SIZE.x {
                    if !lit(&side, &bg, x, y) {
                        continue;
                    }
                    drawn += 1;
                    if view.ground(x, y).is_some_and(|point| point.x < -0.02) {
                        unseen += 1;
                    }
                }
            }
        }
        (drawn, unseen)
    };
    let (side_lit, side_unseen) = beside_take(false);
    let (far_lit, far_unseen) = beside_take(true);
    assert_eq!(
        side_unseen + far_unseen,
        0,
        "{}: nothing drawn over the unseen ground beside the path",
        shot.effect
    );
    // Review 2, E2-1: the positive control. Beyond every layer's reach of the fog, the shot is
    // not cut: it draws over the three ages.
    assert!(
        far_lit >= 30,
        "{}: beyond its reach of the fog the shot draws ({far_lit} pixels)",
        shot.effect
    );
    Ok(format!(
        "shot {} (flight {:.2} s; reach {:.2} at the muzzle, {:.2} along the path): {:.2} beside the fog {side_lit} pixels drawn and {:.2} beside it {far_lit}, none over unseen ground; {count} Critical pixels, at most {widest:.1} px off the path (half width {half_width:.1}); {path}; nothing after its end; under the fog {seen_lit} pixels on the seen half and none on the unseen one",
        shot.effect,
        shot.flight,
        effect.shot_reach.muzzle,
        effect.shot_reach.path,
        beside_offset(effect, false),
        beside_offset(effect, true)
    ))
}

/// Plan WP11: every shipped shot draws along its path and is cut where the fog begins.
#[test]
fn shipped_shots_draw_along_their_paths() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let subjects = shot_subjects();
    assert!(
        subjects.len() >= 8,
        "the shipped units name their shots: {subjects:?}"
    );
    for index in 0..subjects.len() {
        let report = check_shot(index)?;
        eprintln!("{report}");
    }
    Ok(())
}

// --- WP11 round 2: every shipped weapon look can be seen (review 1, E1-5) ----------------------
//
// Every `Shot` and `Point` look a shipped unit names, and the default death, in the production
// lab at the battle camera's default zoom on the brightest and the darkest ground: at its peak
// of the sampled ages, the pixels the whole look changes by at least [`SEEN_LEVELS`] cover at
// least its class's [`seen_minimum`], and its `Critical` layers' median contrast against the
// floor is at least [`SEEN_LEVELS`]. Both are printed per look and ground.

/// Environment variable carrying the look child's subject index.
const LOOK_INDEX_VAR: &str = "SPACE_SCUFFLE_EFFECT_LOOK_INDEX";
/// Environment variable naming, comma-separated, the only looks the visibility test measures
/// (a convenience while tuning; unset, every look is measured).
const LOOK_ONLY_VAR: &str = "SPACE_SCUFFLE_EFFECT_LOOK_ONLY";
/// A pixel counts as seen when its largest channel or its luminance changes by this many levels.
const SEEN_LEVELS: i32 = 32;
/// Least `UnitRadius` the default death is shown with: a Pulse Bot's bubble.
const DEATH_RADIUS: f32 = 0.375;

/// A shipped weapon look, its event kind and the values it binds in the lab.
#[derive(Debug, Clone)]
struct LookSubject {
    effect: String,
    kind: client::effects::catalog::schema::EventKind,
    bound: client::effects::binding::BoundValues,
}

/// Every travel and impact a shipped unit names, and the default death, sorted by name.
fn look_subjects() -> Vec<LookSubject> {
    use client::effects::catalog::schema::EventKind;
    let catalog = catalog();
    let mut names: Vec<String> = sim_fixtures::shipped_catalogs()
        .units
        .archetypes()
        .flat_map(|archetype| archetype.effects.weapons.clone())
        .flat_map(|entry| [entry.travel, entry.impact])
        .flatten()
        .collect();
    names.extend(
        catalog
            .defaults
            .death
            .and_then(|id| catalog.effect(id))
            .map(|effect| effect.name.clone()),
    );
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| {
            let effect = catalog
                .effect(catalog.effect_id(&name).expect("defined"))
                .expect("resolves");
            let flies = effect
                .layers
                .iter()
                .any(|layer| layer.lasts == Some(Binding::FlightTime));
            let bound = match effect.event {
                EventKind::Shot => client::effects::binding::BoundValues::default()
                    .with(Binding::LineLength, SHOT_LENGTH)
                    .with(Binding::FlightTime, if flies { SHOT_FLIGHT } else { 0.0 }),
                EventKind::Death => client::effects::binding::BoundValues::default()
                    .with(Binding::UnitRadius, DEATH_RADIUS)
                    .with(Binding::UnitHeight, DEATH_RADIUS * 2.0),
                // An impact as a battle binds it: struck by an attacker towards -x, on a bubble
                // of a Pulse Bot's radius (its anchor on that bubble's near surface).
                _ => client::effects::binding::BoundValues::default()
                    .with(Binding::UnitRadius, DEATH_RADIUS)
                    .with(Binding::Facing, std::f32::consts::PI),
            };
            LookSubject {
                effect: name,
                kind: effect.event,
                bound,
            }
        })
        .collect()
}

/// The least area in screen pixels a look of `kind` must change by [`SEEN_LEVELS`] at its
/// peak: a shot's slug or body, an impact's flash or splash, a death's burst.
fn seen_minimum(kind: client::effects::catalog::schema::EventKind) -> usize {
    use client::effects::catalog::schema::EventKind;
    match kind {
        EventKind::Shot => 40,
        EventKind::Death => 150,
        _ => 80,
    }
}

/// The ages a look is sampled at: a shot at 0.3, 0.5 and 0.7 of its `Critical` layers' life,
/// anything else in its first frames, where an impact peaks.
fn look_ages(effect: &Effect, subject: &LookSubject) -> [f32; 3] {
    use client::effects::catalog::schema::EventKind;
    if subject.kind != EventKind::Shot {
        return [0.02, 0.06, 0.12];
    }
    let end = effect
        .layers
        .iter()
        .filter(|layer| layer.priority == Priority::Critical)
        .map(|layer| layer.start + layer.span(&subject.bound))
        .fold(0.0, f32::max);
    [end * 0.3, end * 0.5, end * 0.7]
}

/// The lab subject of a look: a shot's line along +x, anything else at the origin.
fn look_lab(subject: &LookSubject) -> LabSubject {
    use client::effects::catalog::schema::EventKind;
    let line = subject.kind == EventKind::Shot;
    LabSubject {
        effect: subject.effect.clone(),
        bound: subject.bound,
        anchor: if line {
            Vec3::new(-SHOT_LENGTH * 0.5, 0.0, 0.0)
        } else {
            Vec3::ZERO
        },
        line_to: line.then_some(Vec3::new(SHOT_LENGTH * 0.5, 0.0, 0.0)),
        line_seen: u64::MAX,
        fog_below_x: None,
        repeat: None,
    }
}

/// The look child: on each ground the background, then the whole look and its `Critical` layers
/// at the sampled ages.
#[test]
#[ignore = "launched by the look visibility test on a virtual display"]
fn effect_look_child() {
    let out = PathBuf::from(std::env::var_os(OUT_VAR).expect("the parent names the directory"));
    let index: usize = std::env::var(LOOK_INDEX_VAR)
        .expect("the parent names the subject")
        .parse()
        .expect("the index is a number");
    let subjects = look_subjects();
    let subject = &subjects[index];
    let catalog = catalog();
    let effect = catalog
        .effect(catalog.effect_id(&subject.effect).expect("defined"))
        .expect("resolves");
    let ages = look_ages(effect, subject);
    let mut takes = Vec::new();
    for (ground, prefix) in [(LabGround::Bright, "bright"), (LabGround::Dark, "dark")] {
        let named = |what: &str| -> Vec<(String, f32)> {
            ages.iter()
                .enumerate()
                .map(|(index, age)| (format!("{prefix}_{what}_{index}"), *age))
                .collect()
        };
        let whole = named("whole");
        let critical = named("critical");
        let shots = |list: &[(String, f32)]| -> Vec<LabShot> {
            list.iter()
                .map(|(name, age)| LabShot {
                    age: *age,
                    overlay: false,
                    name: name.clone(),
                })
                .collect()
        };
        takes.push(LabTake {
            ground,
            zoom: LabZoom::Default,
            effect: false,
            only: None,
            step: 0.25,
            shots: shots(&[(format!("{prefix}_bg"), 0.0)]),
        });
        takes.push(LabTake {
            ground,
            zoom: LabZoom::Default,
            effect: true,
            only: None,
            step: 0.02,
            shots: shots(&whole),
        });
        takes.push(LabTake {
            ground,
            zoom: LabZoom::Default,
            effect: true,
            only: Some(Priority::Critical),
            step: 0.02,
            shots: shots(&critical),
        });
    }
    let program = LabProgram {
        subject: look_lab(subject),
        takes,
        out,
    };
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let mut app = lab_app(root, program, None, SIZE).expect("the lab app builds");
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// How far pixel `(x, y)` of `image` lies from `background`: the larger of its largest channel
/// change and its luminance change, levels.
fn seen_change(image: &RgbImage, background: &RgbImage, x: u32, y: u32) -> i32 {
    let channel = difference(image, background, x, y);
    #[expect(clippy::cast_possible_truncation, reason = "levels below 256")]
    let luma = (luminance(image, x, y) - luminance(background, x, y))
        .abs()
        .round() as i32;
    channel.max(luma)
}

/// The per-channel median of `colours` (review 2, E2-4: the look's own pixels, those its
/// `Critical` layers change by [`SEEN_LEVELS`]); black when there are none.
fn median_colour(colours: &[[u8; 3]]) -> [u8; 3] {
    let mut out = [0; 3];
    for (channel, value) in out.iter_mut().enumerate() {
        let mut values: Vec<u8> = colours.iter().map(|colour| colour[channel]).collect();
        values.sort_unstable();
        *value = values.get(values.len() / 2).copied().unwrap_or(0);
    }
    out
}

/// Runs the look child for subject `index` and measures it; returns its report line.
fn check_look(index: usize, keep: Option<&Path>) -> TestResult<String> {
    let subjects = look_subjects();
    let subject = &subjects[index];
    let directory = tempfile::tempdir()?;
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe()?);
    command.args(["--ignored", "--exact", "--nocapture", "effect_look_child"]);
    command.env(OUT_VAR, directory.path());
    command.env(LOOK_INDEX_VAR, index.to_string());
    let driver = command.driver().map_or("unknown", gpu_run::GpuDriver::name);
    command.assert_passes_validation("look visibility");
    if let Some(keep) = keep {
        let keep = keep.join(driver).join(&subject.effect);
        std::fs::create_dir_all(&keep)?;
        for entry in std::fs::read_dir(directory.path())? {
            let entry = entry?;
            std::fs::copy(entry.path(), keep.join(entry.file_name()))?;
        }
    }
    let minimum = seen_minimum(subject.kind);
    let mut report = format!("look {} on {driver}:", subject.effect);
    for prefix in ["bright", "dark"] {
        let bg = capture(directory.path(), &format!("{prefix}_bg"));
        let mut best_area = 0;
        let mut best_critical: Vec<i32> = Vec::new();
        let mut best_colours: Vec<[u8; 3]> = Vec::new();
        let mut floor: Vec<[u8; 3]> = Vec::new();
        for age in 0..3 {
            let whole = capture(directory.path(), &format!("{prefix}_whole_{age}"));
            let critical = capture(directory.path(), &format!("{prefix}_critical_{age}"));
            let mut area = 0;
            let mut changes = Vec::new();
            let mut colours = Vec::new();
            let mut under = Vec::new();
            for y in 0..SIZE.y {
                for x in 0..SIZE.x {
                    if seen_change(&whole, &bg, x, y) >= SEEN_LEVELS {
                        area += 1;
                    }
                    let change = seen_change(&critical, &bg, x, y);
                    if change > 8 {
                        changes.push(change);
                    }
                    if change >= SEEN_LEVELS {
                        colours.push(critical.get_pixel(x, y).0);
                        under.push(bg.get_pixel(x, y).0);
                    }
                }
            }
            best_area = best_area.max(area);
            if changes.len() > best_critical.len() {
                best_critical = changes;
                best_colours = colours;
                floor = under;
            }
        }
        best_critical.sort_unstable();
        let median = best_critical
            .get(best_critical.len() / 2)
            .copied()
            .unwrap_or(0);
        write!(
            report,
            " {prefix}: {best_area} px seen (least {minimum}), Critical median contrast {median} levels over {} px, display sRGB {:?} over floor {:?};",
            best_critical.len(),
            median_colour(&best_colours),
            median_colour(&floor)
        )?;
        assert!(
            best_area >= minimum,
            "{} on {prefix} ground: {best_area} px change by {SEEN_LEVELS} levels, less than {minimum}",
            subject.effect
        );
        assert!(
            median >= SEEN_LEVELS,
            "{} on {prefix} ground: the Critical layers' median contrast is {median} levels",
            subject.effect
        );
    }
    Ok(report)
}

/// Review 1, E1-5: every shipped weapon look and the default death can be seen at the battle
/// camera's default zoom on both grounds.
#[test]
fn shipped_weapon_looks_can_be_seen() {
    if gpu_run::gpu_tests_opted_out() {
        return;
    }
    let keep = std::env::var_os(KEEP_VAR).map(PathBuf::from);
    let subjects = look_subjects();
    assert!(
        subjects.len() >= 20,
        "the shipped units name their looks: {subjects:?}"
    );
    let mut failures = Vec::new();
    let only = std::env::var(LOOK_ONLY_VAR).ok();
    for (index, subject) in subjects.iter().enumerate() {
        if only
            .as_deref()
            .is_some_and(|only| !only.split(',').any(|name| name == subject.effect))
        {
            continue;
        }
        match std::panic::catch_unwind(|| check_look(index, keep.as_deref())) {
            Ok(Ok(report)) => eprintln!("{report}"),
            Ok(Err(error)) => failures.push(format!("{}: {error}", subject.effect)),
            Err(panic) => failures.push(format!(
                "{}: {}",
                subject.effect,
                panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                    .unwrap_or_default()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "looks that cannot be seen:\n{}",
        failures.join("\n")
    );
}
