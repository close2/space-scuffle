//! The decal aging pass on a real GPU (plan `2026-11-19-retained-effect-instances` WP6, D-5,
//! D-8, D-10).
//!
//! # Responsibility
//! Renders the decal identity scene (`support/decal_identity_scene.rs`) in the production effect
//! lab, offscreen through `support/gpu_run.rs` (radv with the shared lock, or lavapipe), in three
//! child processes:
//! - **gpu**: the production path, the rows aged by `decal_age.wgsl`, with
//!   [`EffectDecalProbe`] reading back every frame's aged decal buffer beside the Rust reference
//!   (`decal_table::age_decal`) of the same rows, time and camera;
//! - **cpu**: [`EffectDecalSource::Cpu`], the reference's output uploaded into the same buffer
//!   and drawn by the same `decal.wgsl`;
//! - **resync**: the production path, but every fifth extraction forgets which generation the
//!   GPU's rows hold, so it copies every live row instead of the patches.
//!
//! Each child's crowd also holds, far behind the camera, the first shipped held look with a held
//! decal layer, released mid-take: its rows are aged every frame but draw no pixel, so the
//! release's exit is compared while the captures stay those of the identity scene. The probe's
//! render systems come from `EffectDecalProbePlugin`, which only tests add.
//!
//! The parent then checks:
//! 1. **Record parity** (D-10 check 1), over every frame of the gpu and resync children: integer
//!    fields equal; float fields within 4 ulp or 1e-6; a flipbook frame index one apart only where
//!    the blend sits within 1e-5 of a whole frame; visibility equal except rows within 1e-5 s of
//!    a start or end or 1e-4 px of the level-of-detail threshold, at most 1 per 10 000 row-frames;
//!    and every branch of the aging (a released held layer, retired detail, retired marks, a
//!    variant, a row hidden by level of detail, and a fitted flipbook when the vocabulary has a
//!    decal on one: the shipped one has none) occurs among the rows.
//! 2. **Pixel identity in the same run** (check 2): every gpu capture against the cpu capture of
//!    the same shot has ≥ 99.9 % of pixels bit-identical, every channel within 2/255 and a mean
//!    absolute difference ≤ 0.02/255.
//! 3. **The full resync**: the resync child's captures equal the gpu child's bit for bit.
//!
//! The scene is rendered by separate children rather than twice in one child: two takes in one
//! process start at different absolute `Time<Virtual>` and move the lab's steps by nanoseconds
//! (plan Amendment 1, decision 2), while two processes render the same take identically
//! (`decal_identity_gpu_tests`, D-9).
//!
//! With [`BASE_VAR`] naming a directory of the identity scene's earlier captures (`a_NN.png`, WP1
//! and WP5 evidence), a fourth child, **plain**, renders the identity scene without the further
//! look below, aged on the GPU, and its captures are held to check 2's numbers against those
//! (check 3) and the figures printed; with [`SAVE_VAR`] the captures are copied there for the evidence.
//!
//! # Invariants verified
//! As listed; the parity and pixel figures are printed for the evidence.

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

#[allow(dead_code, reason = "shared with the decal identity tests")]
#[path = "support/decal_identity_scene.rs"]
mod decal_identity_scene;

use bevy::math::Vec3;
use client::effects::catalog::EffectCatalog;
use client::effects::catalog::{Block, Pool};
use client::effects::decal_table::decal_clock;
use client::effects::decal_table::{age_margins, hidden_decal};
use client::effects::instances::DecalInstance;
use client::effects::lab::LabCrowdMember;
use client::effects::lab::{lab_app, LabSurface};
use client::effects::render::{
    DecalProbeFrame, EffectDecalProbe, EffectDecalProbePlugin, EffectDecalSource,
};
use client::ResolvedAssetsRoot;
use decal_identity_scene::{
    bound_of, held_kind, scene, shipped, subject, tightened_vocabulary, SHOTS, SIZE,
};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying the child's output directory.
const OUT_VAR: &str = "SPACE_SCUFFLE_DECAL_AGE_OUT";
/// Environment variable carrying the child's mode.
const MODE_VAR: &str = "SPACE_SCUFFLE_DECAL_AGE_MODE";
/// Environment variable naming a directory of earlier identity captures to compare with.
const BASE_VAR: &str = "SPACE_SCUFFLE_DECAL_AGE_BASE";
/// Environment variable naming a directory the captures are copied into.
const SAVE_VAR: &str = "SPACE_SCUFFLE_DECAL_AGE_SAVE";
/// The children, in the order they run.
const MODES: [&str; 3] = ["gpu", "cpu", "resync"];
/// The child that renders the identity scene exactly as `decal_identity_gpu_tests` does (no
/// further look), aged on the GPU: run only with [`BASE_VAR`], whose captures it is compared with
/// (the released held look the other children add shifts the spawn sequence, which seeds every
/// later effect's randomness, so their captures are not the identity scene's).
const PLAIN: &str = "plain";
/// Every this-many-th extraction of the resync child copies every row.
const FORGET_EVERY: u32 = 5;
/// Float fields agree within this many ulp ...
const MAX_ULPS: u32 = 4;
/// ... or this much absolutely.
const MAX_ABSOLUTE: f32 = 1e-6;
/// A flipbook frame index may differ by one where the blend is this near a whole frame.
const FRAME_EDGE: f32 = 1e-5;
/// A visibility flip is a boundary flip within this many seconds of a start or end ...
const EDGE_SECONDS: f32 = 1e-5;
/// ... or this many pixels of the level-of-detail threshold.
const EDGE_PIXELS: f32 = 1e-4;
/// Words of [`DecalInstance`] that hold integers (`layer_a`, `layer_b`, `ramp_row`,
/// `noise_layer`); the rest are `f32`.
const INTEGER_WORDS: [usize; 4] = [8, 9, 11, 23];
/// Words of `layer_a`, `layer_b` and `frame_mix`.
const FRAME_WORDS: [usize; 3] = [8, 9, 10];

/// The child: renders the identity scene in its mode, and in the gpu and resync modes writes the
/// parity report of every frame.
#[test]
#[ignore = "launched by the decal aging test through gpu_run, offscreen"]
fn decal_age_child() {
    let out = PathBuf::from(std::env::var_os(OUT_VAR).expect("the parent names the directory"));
    let mode = std::env::var(MODE_VAR).expect("the parent names the mode");
    let vocabulary = tightened_vocabulary(&out);
    let (program, mut crowd) = scene(out.clone(), &mode);
    if mode != PLAIN {
        crowd.members.push(released_held_member());
    }
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let mut app = lab_app(
        root.clone(),
        program,
        Some(vocabulary.clone()),
        SIZE,
        LabSurface::Offscreen,
    )
    .expect("the lab app builds");
    app.insert_resource(crowd);
    let probe = EffectDecalProbe {
        read_back: mode != "cpu" && mode != PLAIN,
        forget_every: if mode == "resync" { FORGET_EVERY } else { 0 },
        ..EffectDecalProbe::default()
    };
    let frames = probe.frames.clone();
    app.insert_resource(probe);
    app.add_plugins(EffectDecalProbePlugin);
    if mode == "cpu" {
        app.insert_resource(EffectDecalSource::Cpu);
    }
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
    if mode == "cpu" || mode == PLAIN {
        return;
    }
    let catalog = EffectCatalog::load(&vocabulary, root.as_path()).expect("the copy loads");
    let frames = frames.lock().expect("the probe's frames");
    let mut report = parity(&frames, &catalog);
    report.flipbook_looks = catalog
        .effects
        .iter()
        .flat_map(|effect| &effect.layers)
        .filter(|layer| match &layer.block {
            Block::Decal(look) => !look.element.is_variant_set(),
            _ => false,
        })
        .count();
    std::fs::write(out.join(format!("{mode}_parity.txt")), report.render())
        .expect("the report writes");
}

/// Where the released held look stands: far behind the battle camera (which looks towards
/// -z), so its rows are aged and compared every frame (its depth is negative, so the level of
/// detail never hides it) while it draws no pixel of the captures, which stay comparable with
/// the identity scene's earlier ones.
const BEHIND_CAMERA: Vec3 = Vec3::new(0.0, 0.0, 2000.0);
/// When the released held look is held, seconds.
const HELD_AT: f32 = 0.1;
/// When it is released, seconds.
const RELEASED_AT: f32 = 4.5;

/// The first shipped held look with a held decal layer (found from the catalog, none named),
/// held at [`HELD_AT`] and released at [`RELEASED_AT`] (independent review round 2, S1: the
/// identity scene releases no held decal, so the exit branch of the aging went untested).
fn released_held_member() -> LabCrowdMember {
    let catalog = shipped();
    let effect = catalog
        .effects
        .iter()
        .find(|effect| {
            held_kind(effect.event)
                && effect
                    .layers
                    .iter()
                    .any(|layer| layer.held && layer.block.pool() == Pool::Decals)
        })
        .expect("a shipped held look with a held decal layer");
    let splashes = effect_subjects::splash_subjects();
    LabCrowdMember {
        subject: subject(
            &effect.name,
            effect.event,
            BEHIND_CAMERA,
            bound_of(&effect.name, &splashes),
        ),
        at: HELD_AT,
        held: true,
        release_at: Some(RELEASED_AT),
        velocity: Vec3::ZERO,
        fog_sweep: None,
    }
}

/// Drawn row-frames per branch of the aging, so that the parity is known to have run each.
#[derive(Debug, Default)]
struct Branches {
    /// A held layer after its release (`exit`, `exit_floor`).
    released_held: usize,
    /// A `Detail` layer fading after its effect's detail was retired.
    detail_retired: usize,
    /// A mark fading after its effect's marks were retired.
    marks_retired: usize,
    /// A flipbook fitted to the life.
    flipbook: usize,
    /// A variant set (the frame fixed in the template).
    variant: usize,
    /// Rows with a clock hidden by the level of detail.
    lod_hidden: usize,
}

/// What the record parity found over a child's frames.
#[derive(Debug, Default)]
struct Parity {
    /// Frames read back.
    frames: usize,
    /// Of them, frames that copied every row.
    full_copies: usize,
    /// Rows compared over all frames.
    row_frames: usize,
    /// Rows drawn by both.
    drawn: usize,
    /// Rows bit-identical.
    identical: usize,
    /// Largest ulp distance of a float field beyond `MAX_ABSOLUTE`.
    max_ulps: u32,
    /// Flipbook frames one apart at a whole frame.
    frame_flips: usize,
    /// Visibility flips at a boundary.
    boundary_flips: usize,
    /// Drawn row-frames per branch.
    branches: Branches,
    /// Decal layers of the vocabulary whose element is a fitted flipbook, not a variant set
    /// (the shipped vocabulary has none, so its flipbook branch is drawn by no row).
    flipbook_looks: usize,
    /// Rows outside the tolerance, described.
    failures: Vec<String>,
}

impl Parity {
    /// The report as `key value` lines, failures last.
    fn render(&self) -> String {
        let mut text = format!(
            "frames {}\nfull_copies {}\nrow_frames {}\ndrawn {}\nidentical {}\nmax_ulps {}\n\
             frame_flips {}\nboundary_flips {}\nreleased_held {}\ndetail_retired {}\n\
             marks_retired {}\nflipbook {}\nvariant {}\nlod_hidden {}\nflipbook_looks {}\n\
             failures {}\n",
            self.frames,
            self.full_copies,
            self.row_frames,
            self.drawn,
            self.identical,
            self.max_ulps,
            self.frame_flips,
            self.boundary_flips,
            self.branches.released_held,
            self.branches.detail_retired,
            self.branches.marks_retired,
            self.branches.flipbook,
            self.branches.variant,
            self.branches.lod_hidden,
            self.flipbook_looks,
            self.failures.len()
        );
        for failure in self.failures.iter().take(20) {
            writeln!(text, "failure {failure}").expect("a string takes text");
        }
        text
    }
}

/// The ulp distance of two finite `f32` of the same sign, or `u32::MAX`.
fn ulps(a: f32, b: f32) -> u32 {
    if a.is_sign_negative() != b.is_sign_negative() {
        // +0 and -0 are the one pair of equal values of different signs.
        return if a.abs().to_bits() == 0 && b.abs().to_bits() == 0 {
            0
        } else {
            u32::MAX
        };
    }
    a.to_bits().abs_diff(b.to_bits())
}

/// D-10 check 1 over `frames`.
fn parity(frames: &[DecalProbeFrame], catalog: &EffectCatalog) -> Parity {
    let mut report = Parity::default();
    let hidden = hidden_decal();
    for frame in frames {
        report.frames += 1;
        report.full_copies += usize::from(frame.full_copy);
        assert_eq!(frame.aged.len(), frame.expected.len(), "one decal per row");
        for ((record, expected), aged) in frame.records.iter().zip(&frame.expected).zip(&frame.aged)
        {
            report.row_frames += 1;
            let (expected_drawn, aged_drawn) = (*expected != hidden, *aged != hidden);
            if expected_drawn != aged_drawn {
                let margins = age_margins(record, frame.now, catalog, frame.camera.as_ref());
                if margins.seconds < EDGE_SECONDS || margins.pixels < EDGE_PIXELS {
                    report.boundary_flips += 1;
                } else {
                    report.failures.push(format!(
                        "at {:.4} s slot {}: drawn {expected_drawn} by the reference, \
                         {aged_drawn} by the GPU ({margins:?})",
                        frame.now, record.slot
                    ));
                }
                continue;
            }
            if !expected_drawn {
                if decal_clock(record, frame.now).is_some() {
                    report.branches.lod_hidden += 1;
                }
                continue;
            }
            report.drawn += 1;
            let branches = &mut report.branches;
            branches.released_held += usize::from(record.held && record.released_at.is_some());
            branches.detail_retired +=
                usize::from(record.detail && record.detail_retired_at.is_some());
            branches.marks_retired += usize::from(record.marks_retired_at.is_some());
            branches.flipbook += usize::from(record.flipbook.is_some());
            branches.variant += usize::from(record.flipbook.is_none());
            if expected == aged {
                report.identical += 1;
                continue;
            }
            let failures = report.failures.len();
            compare(expected, aged, &mut report, (frame.now, record.slot));
            if report.failures.len() > failures {
                report.failures.push(format!(
                    "  its record: birth {:?} start {} delay {} span {} held {} released {:?} \
                     detail {} detail retired {:?} marks retired {:?} now {:?} curve {:?} \
                     strength {:?}",
                    record.birth,
                    record.start,
                    record.delay,
                    record.span,
                    record.held,
                    record.released_at,
                    record.detail,
                    record.detail_retired_at,
                    record.marks_retired_at,
                    frame.now,
                    record
                        .curve
                        .and_then(|id| catalog.curves.get(usize::from(id.0)))
                        .map(|curve| curve.keys().to_vec()),
                    record.strength
                ));
            }
        }
    }
    report
}

/// Compares two drawn decals field by field into `report`.
fn compare(
    expected: &DecalInstance,
    aged: &DecalInstance,
    report: &mut Parity,
    (now, slot): (f64, u32),
) {
    let words = |decal: &DecalInstance| -> [u32; 36] { bytemuck::cast(*decal) };
    let (want, got) = (words(expected), words(aged));
    let near_whole = |mix: f32| mix < FRAME_EDGE || mix > 1.0 - FRAME_EDGE;
    let frame_flip = want[8].abs_diff(got[8]) == 1
        && (near_whole(expected.frame_mix) || near_whole(aged.frame_mix));
    if frame_flip {
        report.frame_flips += 1;
    }
    for word in 0..36 {
        if frame_flip && FRAME_WORDS.contains(&word) {
            continue;
        }
        if want[word] == got[word] {
            continue;
        }
        if INTEGER_WORDS.contains(&word) {
            report.failures.push(format!(
                "at {now:.4} s slot {slot}: word {word} is {} against {}",
                got[word], want[word]
            ));
            continue;
        }
        let (a, b) = (f32::from_bits(want[word]), f32::from_bits(got[word]));
        if (a - b).abs() <= MAX_ABSOLUTE {
            continue;
        }
        let distance = ulps(a, b);
        if distance <= MAX_ULPS {
            report.max_ulps = report.max_ulps.max(distance);
            continue;
        }
        report.failures.push(format!(
            "at {now:.4} s slot {slot}: word {word} is {b} against {a} ({distance} ulp)"
        ));
    }
}

/// Reads value `name` from a parity report.
fn value(report: &str, name: &str) -> usize {
    report
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(' ')?;
            (key == name).then(|| value.parse().ok()).flatten()
        })
        .unwrap_or_else(|| panic!("no {name} in the report:\n{report}"))
}

/// Pixel comparison of two captures (D-10 check 2's numbers).
#[derive(Debug, Clone, Copy, PartialEq)]
struct PixelDiff {
    /// Share of pixels bit-identical.
    identical: f64,
    /// Largest channel difference, levels of 255.
    max_levels: u8,
    /// Mean absolute channel difference, levels of 255.
    mean_levels: f64,
}

impl PixelDiff {
    /// Whether it is within check 2's tolerance.
    fn within(self) -> bool {
        self.identical >= 0.999 && self.max_levels <= 2 && self.mean_levels <= 0.02
    }
}

/// Compares the captures at `a` and `b`.
fn pixel_diff(a: &Path, b: &Path) -> TestResult<PixelDiff> {
    let (a, b) = (image::open(a)?.to_rgb8(), image::open(b)?.to_rgb8());
    assert_eq!(a.dimensions(), b.dimensions(), "captures of one size");
    let (mut identical, mut max_levels, mut sum) = (0_u64, 0_u8, 0_u64);
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        identical += u64::from(pa == pb);
        for channel in 0..3 {
            let difference = pa.0[channel].abs_diff(pb.0[channel]);
            max_levels = max_levels.max(difference);
            sum += u64::from(difference);
        }
    }
    let pixels = f64::from(a.width()) * f64::from(a.height());
    Ok(PixelDiff {
        identical: identical as f64 / pixels,
        max_levels,
        mean_levels: sum as f64 / (pixels * 3.0),
    })
}

/// The parity of `mode`'s child, printed and held to D-10 check 1.
fn assert_parity(directory: &Path, mode: &str, driver: &str) -> TestResult<String> {
    let report = std::fs::read_to_string(directory.join(format!("{mode}_parity.txt")))?;
    let (rows, flips, failures) = (
        value(&report, "row_frames"),
        value(&report, "boundary_flips"),
        value(&report, "failures"),
    );
    println!("{driver}: {mode} record parity:\n{report}");
    assert!(
        value(&report, "frames") > 50,
        "{driver} {mode}: frames read back"
    );
    assert!(
        value(&report, "drawn") > 1000,
        "{driver} {mode}: rows drawn"
    );
    let flipbook = value(&report, "flipbook_looks") > 0;
    for branch in [
        "released_held",
        "detail_retired",
        "marks_retired",
        "variant",
        "lod_hidden",
    ]
    .into_iter()
    .chain(flipbook.then_some("flipbook"))
    {
        assert!(
            value(&report, branch) > 0,
            "{driver} {mode}: the parity covers the {branch} branch\n{report}"
        );
    }
    assert_eq!(
        failures, 0,
        "{driver} {mode}: rows outside the tolerance\n{report}"
    );
    assert!(
        flips * 10_000 <= rows,
        "{driver} {mode}: {flips} boundary flips in {rows} row-frames"
    );
    Ok(report)
}

/// D-10 checks 1 and 2 on the identity scene, and the full resync draws what the patches draw.
#[test]
fn the_gpu_ages_the_decal_rows_as_the_reference_does() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let mut driver = None;
    let base = std::env::var_os(BASE_VAR).map(PathBuf::from);
    let plain: &[&str] = if base.is_some() { &[PLAIN] } else { &[] };
    for &mode in MODES.iter().chain(plain) {
        let mut command = gpu_run::offscreen_gpu_command(&std::env::current_exe()?);
        command.args(["--ignored", "--exact", "--nocapture", "decal_age_child"]);
        command.env(OUT_VAR, directory.path());
        command.env(MODE_VAR, mode);
        driver = command.driver();
        command.assert_passes_validation(&format!("decal aging, {mode} child"));
    }
    let driver = driver.map_or("unknown", gpu_run::GpuDriver::name);
    let path = |mode: &str, shot: usize| directory.path().join(format!("{mode}_{shot:02}.png"));
    assert_parity(directory.path(), "gpu", driver)?;
    let resync = assert_parity(directory.path(), "resync", driver)?;
    assert!(
        value(&resync, "full_copies") >= 10,
        "{driver}: the resync child copied every row repeatedly"
    );
    let mut worst: Option<PixelDiff> = None;
    for shot in 0..SHOTS {
        let against_cpu = pixel_diff(&path("gpu", shot), &path("cpu", shot))?;
        println!("{driver}: capture {shot:02} gpu against cpu: {against_cpu:?}");
        assert!(
            against_cpu.within(),
            "{driver}: capture {shot} of the GPU aging is outside D-10 check 2 against the CPU \
             reference: {against_cpu:?}"
        );
        if worst.is_none_or(|worst| against_cpu.identical < worst.identical) {
            worst = Some(against_cpu);
        }
        let resynced = pixel_diff(&path("gpu", shot), &path("resync", shot))?;
        assert!(
            resynced.max_levels == 0,
            "{driver}: capture {shot} after full copies differs from the patched one: \
             {resynced:?}"
        );
    }
    println!("{driver}: worst capture gpu against cpu: {worst:?}");
    if let Some(base) = &base {
        for shot in 0..SHOTS {
            let against_base =
                pixel_diff(&path(PLAIN, shot), &base.join(format!("a_{shot:02}.png")))?;
            println!("{driver}: capture {shot:02} plain against the base: {against_base:?}");
            assert!(
                against_base.within(),
                "{driver}: capture {shot} is outside D-10 check 2 against the base: \
                 {against_base:?}"
            );
        }
    }
    if let Some(save) = std::env::var_os(SAVE_VAR) {
        let target = PathBuf::from(save).join(driver);
        std::fs::create_dir_all(&target)?;
        for &mode in MODES.iter().chain(plain) {
            for shot in 0..SHOTS {
                std::fs::copy(
                    path(mode, shot),
                    target.join(format!("{mode}_{shot:02}.png")),
                )?;
            }
        }
        for mode in ["gpu", "resync"] {
            let name = format!("{mode}_parity.txt");
            std::fs::copy(directory.path().join(&name), target.join(name))?;
        }
    }
    Ok(())
}
