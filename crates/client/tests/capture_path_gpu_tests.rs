//! The capture path does not change the pictures: an offscreen lab capture equals its window
//! capture, pixel for pixel (plan `docs/plans/2026-11-17-gpu-test-readback.md`, D-6, test T-1).
//!
//! # Responsibility
//! One program of the production effect lab (`client::effects::lab`) runs twice, as two children
//! of this binary through `support/gpu_run.rs` on the selected driver (radv with the shared lock,
//! or lavapipe; charter C21):
//! - [`LabSurface::Window`] on a virtual display (`xvfb-run`; on radv with
//!   `MESA_VK_WSI_DEBUG=sw`), the path every effect child took before the plan;
//! - [`LabSurface::Offscreen`] with no display: the camera draws into a canvas image and each
//!   screenshot copies the canvas (`client::offscreen`).
//!
//! The program is the first splash subject of the shipped units (sorted by effect name, as the
//! slice tests sort them; no effect is named here): stills at 0.08, 0.3 and 0.75 of its lifetime
//! and the first three 40 ms frames of its sequence, at 1280×720 on the bright ground under the
//! battle camera's default zoom. The parent requires every capture of both children to have the
//! same pixels. The plan admits a difference of at most 1/255 only with a recorded cause, and none
//! is recorded, so any difference fails.
//!
//! # Invariants verified
//! - Both children write the same set of captures, and each pair is bit-identical in RGB.
//! - The harness records the window child's display as `xvfb-sw` or `xvfb` and the offscreen
//!   child's as `offscreen`.

#![allow(clippy::expect_used, clippy::panic)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/gpu_run.rs"]
mod gpu_run;

#[allow(dead_code, reason = "shared with the catalog and slice tests")]
#[path = "support/effect_subjects.rs"]
mod effect_subjects;

use bevy::math::{UVec2, Vec3};
use client::effects::catalog::schema::Binding;
use client::effects::catalog::{EffectCatalog, EFFECTS_CATALOG_PATH};
use client::effects::lab::program::sequence_take;
use client::effects::lab::{
    lab_app, LabGround, LabProgram, LabShot, LabSubject, LabSurface, LabTake, LabZoom,
    SEQUENCE_STEP,
};
use client::ResolvedAssetsRoot;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Environment variable carrying a child's output directory.
const OUT_VAR: &str = "SPACE_SCUFFLE_CAPTURE_PATH_OUT";
/// Environment variable that keeps both children's captures in a directory for inspection.
const KEEP_VAR: &str = "SPACE_SCUFFLE_CAPTURE_PATH_KEEP";
/// Capture size: the slice tests' and the contact sheets' 1280×720.
const SIZE: UVec2 = UVec2::new(1280, 720);
/// Still ages, fractions of the effect's lifetime: the slice tests' golden ages.
const STILL_FRACTIONS: [f32; 3] = [0.08, 0.3, 0.75];
/// Frames of the sequence captured, from its first.
const SEQUENCE_FRAMES: usize = 3;
/// Libtest name of the window child.
const WINDOW_CHILD: &str = "capture_path_window_child";
/// Libtest name of the offscreen child.
const OFFSCREEN_CHILD: &str = "capture_path_offscreen_child";

/// The lab subject of the first splash subject, laid out as the slice tests lay it out: a point
/// at the origin, or a line of its bound length along +x centred on the origin.
fn first_subject() -> LabSubject {
    let subjects = effect_subjects::splash_subjects();
    let subject = subjects
        .first()
        .expect("the shipped units name at least one splash effect");
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

/// The program both children run, writing into `out`.
fn program(out: PathBuf) -> LabProgram {
    let subject = first_subject();
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let lifetime = catalog
        .effect_id(&subject.effect)
        .and_then(|id| catalog.effect(id))
        .expect("the subject's effect resolves")
        .lifetime;
    let stills = LabTake {
        ground: LabGround::Bright,
        zoom: LabZoom::Default,
        effect: true,
        only: None,
        step: SEQUENCE_STEP,
        shots: STILL_FRACTIONS
            .iter()
            .enumerate()
            .map(|(index, fraction)| LabShot {
                age: fraction * lifetime,
                overlay: false,
                name: format!("still_{index}"),
            })
            .collect(),
    };
    let mut sequence = sequence_take(LabGround::Bright, LabZoom::Default, "seq");
    sequence.shots.truncate(SEQUENCE_FRAMES);
    LabProgram {
        subject,
        takes: vec![stills, sequence],
        out,
    }
}

/// A child: runs the program on `surface`.
fn run_child(surface: LabSurface) {
    let out = PathBuf::from(std::env::var_os(OUT_VAR).expect("the parent names the directory"));
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let mut app = lab_app(root, program(out), None, SIZE, surface).expect("the lab app builds");
    assert!(client::render_compatibility::run_render_app(&mut app).is_success());
}

/// The window child: today's capture path.
#[test]
#[ignore = "launched by an_offscreen_lab_capture_equals_its_window_capture on a virtual display"]
fn capture_path_window_child() {
    run_child(LabSurface::Window);
}

/// The offscreen child: the canvas capture path.
#[test]
#[ignore = "launched by an_offscreen_lab_capture_equals_its_window_capture with no display"]
fn capture_path_offscreen_child() {
    run_child(LabSurface::Offscreen);
}

/// The RGB pixels of every PNG in `directory`, by file name.
fn captures(directory: &Path) -> TestResult<BTreeMap<String, image::RgbImage>> {
    let mut images = BTreeMap::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "png") {
            let name = path
                .file_name()
                .ok_or("a capture has a file name")?
                .to_string_lossy()
                .into_owned();
            images.insert(name, image::open(&path)?.to_rgb8());
        }
    }
    Ok(images)
}

/// How two captures differ: the pixels whose channels differ and the largest channel difference.
fn difference(window: &image::RgbImage, offscreen: &image::RgbImage) -> Option<(usize, u8)> {
    if window.dimensions() != offscreen.dimensions() {
        return Some((usize::MAX, u8::MAX));
    }
    let mut pixels = 0;
    let mut largest = 0;
    for (a, b) in window.pixels().zip(offscreen.pixels()) {
        let channel =
            a.0.iter()
                .zip(b.0)
                .map(|(x, y)| x.abs_diff(y))
                .max()
                .unwrap_or(0);
        if channel > 0 {
            pixels += 1;
            largest = largest.max(channel);
        }
    }
    (pixels > 0).then_some((pixels, largest))
}

/// Copies both children's captures to `<keep>/<driver>/{window,offscreen}`.
fn keep_captures(keep: &Path, driver: &str, window: &Path, offscreen: &Path) -> TestResult {
    for (name, directory) in [("window", window), ("offscreen", offscreen)] {
        let target = keep.join(driver).join(name);
        std::fs::create_dir_all(&target)?;
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// T-1: the offscreen lab capture equals the window capture on the selected driver.
#[test]
fn an_offscreen_lab_capture_equals_its_window_capture() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let window_dir = tempfile::tempdir()?;
    let mut window = gpu_run::virtual_gpu_command(&exe);
    window.args(["--ignored", "--exact", "--nocapture", WINDOW_CHILD]);
    window.env(OUT_VAR, window_dir.path());
    let driver = window.driver().map_or("unknown", gpu_run::GpuDriver::name);
    let window_run = window.assert_passes_validation("capture path, window");
    let offscreen_dir = tempfile::tempdir()?;
    let mut offscreen = gpu_run::offscreen_gpu_command(&exe);
    offscreen.args(["--ignored", "--exact", "--nocapture", OFFSCREEN_CHILD]);
    offscreen.env(OUT_VAR, offscreen_dir.path());
    let offscreen_run = offscreen.assert_passes_validation("capture path, offscreen");
    assert!(
        window_run.ledger.contains("\tdisplay=xvfb")
            || window_run.ledger.contains("\tdisplay=host\t"),
        "the window child ran on a virtual display: {}",
        window_run.ledger
    );
    assert!(
        offscreen_run.ledger.contains("\tdisplay=offscreen\t"),
        "the offscreen child ran with no display: {}",
        offscreen_run.ledger
    );
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        keep_captures(
            Path::new(&keep),
            driver,
            window_dir.path(),
            offscreen_dir.path(),
        )?;
    }
    let window_captures = captures(window_dir.path())?;
    let offscreen_captures = captures(offscreen_dir.path())?;
    assert_eq!(
        window_captures.len(),
        STILL_FRACTIONS.len() + SEQUENCE_FRAMES,
        "the window child wrote every capture: {:?}",
        window_captures.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        window_captures.keys().collect::<Vec<_>>(),
        offscreen_captures.keys().collect::<Vec<_>>(),
        "both children wrote the same captures"
    );
    let mut differences = Vec::new();
    for (name, window_image) in &window_captures {
        let offscreen_image = &offscreen_captures[name];
        if let Some((pixels, largest)) = difference(window_image, offscreen_image) {
            differences.push(format!(
                "{name}: {pixels} pixels differ, by up to {largest}/255"
            ));
        }
    }
    eprintln!(
        "capture path on {driver}: {} captures compared, {} differ",
        window_captures.len(),
        differences.len()
    );
    assert!(
        differences.is_empty(),
        "offscreen captures differ from the window captures on {driver} (set {KEEP_VAR} to keep \
         both):\n{}",
        differences.join("\n")
    );
    Ok(())
}
