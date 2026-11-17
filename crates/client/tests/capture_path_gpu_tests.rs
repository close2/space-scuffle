//! The capture path and the display do not change the pictures: an offscreen lab capture equals
//! its window capture (test T-1), and a window capture through the headless `KWin` equals one
//! through Xvfb (test T-2), pixel for pixel (plan `docs/plans/2026-11-17-gpu-test-readback.md`,
//! D-4 and D-6).
//!
//! # Responsibility
//! One program of the production effect lab (`client::effects::lab`) runs as children of this
//! binary through `support/gpu_run.rs` on the selected driver (radv with the shared lock, or
//! lavapipe; charter C21):
//! - [`LabSurface::Window`] on Xvfb (`xvfb-run`; on radv with `MESA_VK_WSI_DEBUG=sw`), the path
//!   every child took before the plan, forced for this child whatever the process's display;
//! - [`LabSurface::Window`] on the headless `KWin` (`kwin_wayland --virtual --xwayland`), where
//!   radv presents through DRI3 (radv only: lavapipe never uses `KWin`);
//! - [`LabSurface::Offscreen`] with no display: the camera draws into a canvas image and each
//!   screenshot copies the canvas (`client::offscreen`).
//!
//! The program is the first splash subject of the shipped units (sorted by effect name, as the
//! slice tests sort them; no effect is named here): the empty ground, stills at 0.08, 0.3 and
//! 0.75 of its lifetime and the first three 40 ms frames of its sequence, at 1280×720 on the
//! bright ground under the battle camera's default zoom. The parent requires every capture of
//! two children to have the same pixels. The plan admits a difference of at most 1/255 only with
//! a recorded cause, and none is recorded, so any difference fails.
//!
//! The lab program is the witness of T-2 because it is reproducible on Xvfb: its time advances
//! by fixed steps, not by the frame interval. The production skirmish children are not: two Xvfb
//! runs of the concealment child differ in about 8,000 to 10,000 pixels by up to 76/255, and
//! WP1 found the same for the status child and the editor (readback evidence, M4). Those
//! children keep their own checks on every display.
//!
//! # Invariants verified
//! - Both children write the same set of captures, and each pair is bit-identical in RGB.
//! - The captures show something: none is a single colour, and each still of the effect differs
//!   from the empty ground in at least [`MIN_EFFECT_PIXELS`] pixels (WP2 review, F3).
//! - The harness records the Xvfb child's display as `xvfb-sw` or `xvfb` (never `host`: the
//!   display is forced), the `KWin` child's as `kwin` and the offscreen child's as `offscreen`.

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
/// Name of the capture of the empty ground.
const GROUND_SHOT: &str = "ground";
/// A pixel counts as changed by the effect when a channel moves by more than this, in levels of
/// 255.
const EFFECT_CHANGE_LEVEL: u8 = 8;
/// Pixels each still of the effect must change against the empty ground.
const MIN_EFFECT_PIXELS: usize = 500;

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
    let ground = LabTake {
        ground: LabGround::Bright,
        zoom: LabZoom::Default,
        effect: false,
        only: None,
        step: SEQUENCE_STEP,
        shots: vec![LabShot {
            age: 0.0,
            overlay: false,
            name: GROUND_SHOT.to_owned(),
        }],
    };
    LabProgram {
        subject,
        takes: vec![ground, stills, sequence],
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

/// The window child: on Xvfb, today's capture path, and on the headless `KWin`.
#[test]
#[ignore = "launched by the capture path tests on a virtual display"]
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

/// Copies the children's captures to `<keep>/<driver>/<name>` for each `(name, directory)`.
fn keep_captures(keep: &Path, driver: &str, children: &[(&str, &Path)]) -> TestResult {
    for &(name, directory) in children {
        let target = keep.join(driver).join(name);
        std::fs::create_dir_all(&target)?;
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Number of PNGs every child writes: the ground, the stills and the sequence frames.
const CAPTURES: usize = 1 + STILL_FRACTIONS.len() + SEQUENCE_FRAMES;

/// Runs the window child on the display `request` forces, under validation, into a new
/// directory, and checks the ledger names one of `displays`.
fn window_child(
    request: gpu_run::DisplayRequest,
    label: &str,
    displays: &[&str],
) -> TestResult<(tempfile::TempDir, gpu_run::GpuRun)> {
    let directory = tempfile::tempdir()?;
    let mut window = gpu_run::virtual_gpu_command_with(&std::env::current_exe()?, None, request);
    window.args(["--ignored", "--exact", "--nocapture", WINDOW_CHILD]);
    window.env(OUT_VAR, directory.path());
    let run = window.assert_passes_validation(label);
    assert!(
        displays
            .iter()
            .any(|display| run.ledger.contains(&format!("\tdisplay={display}\t"))),
        "{label} ran on one of {displays:?}: {}",
        run.ledger
    );
    Ok((directory, run))
}

/// Checks that the captures show the program (F3): none is one colour, and each still differs
/// from the empty ground in at least [`MIN_EFFECT_PIXELS`] pixels. Prints the figures.
fn assert_shows_the_effect(captures: &BTreeMap<String, image::RgbImage>, path: &str) {
    let ground = &captures[&format!("{GROUND_SHOT}.png")];
    for (name, picture) in captures {
        let first = picture.pixels().next().map(|pixel| pixel.0);
        assert!(
            picture.pixels().any(|pixel| Some(pixel.0) != first),
            "{path}: {name} is a single colour"
        );
        if name.as_str() == format!("{GROUND_SHOT}.png") {
            continue;
        }
        let changed = picture
            .pixels()
            .zip(ground.pixels())
            .filter(|(a, b)| {
                a.0.iter()
                    .zip(b.0)
                    .any(|(x, y)| x.abs_diff(y) > EFFECT_CHANGE_LEVEL)
            })
            .count();
        eprintln!("{path}: {name} changes {changed} pixels of the empty ground");
        if name.starts_with("still_") {
            assert!(
                changed >= MIN_EFFECT_PIXELS,
                "{path}: {name} changes only {changed} pixels of the empty ground"
            );
        }
    }
}

/// Compares two children's captures and fails on any difference.
fn assert_identical(first: (&str, &Path), second: (&str, &Path), driver: &str) -> TestResult {
    let first_captures = captures(first.1)?;
    let second_captures = captures(second.1)?;
    assert_eq!(
        first_captures.len(),
        CAPTURES,
        "the {} child wrote every capture: {:?}",
        first.0,
        first_captures.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        first_captures.keys().collect::<Vec<_>>(),
        second_captures.keys().collect::<Vec<_>>(),
        "both children wrote the same captures"
    );
    assert_shows_the_effect(&first_captures, first.0);
    let mut differences = Vec::new();
    for (name, first_image) in &first_captures {
        if let Some((pixels, largest)) = difference(first_image, &second_captures[name]) {
            differences.push(format!(
                "{name}: {pixels} pixels differ, by up to {largest}/255"
            ));
        }
    }
    eprintln!(
        "{} against {} on {driver}: {} captures compared, {} differ",
        first.0,
        second.0,
        first_captures.len(),
        differences.len()
    );
    assert!(
        differences.is_empty(),
        "{} captures differ from the {} captures on {driver} (set {KEEP_VAR} to keep both):\n{}",
        second.0,
        first.0,
        differences.join("\n")
    );
    Ok(())
}

/// T-1: the offscreen lab capture equals the Xvfb window capture on the selected driver.
#[test]
fn an_offscreen_lab_capture_equals_its_window_capture() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let driver = gpu_run::selected_driver().driver.name();
    let (window_dir, _) = window_child(
        gpu_run::DisplayRequest::Xvfb,
        "capture path, window on Xvfb",
        &["xvfb-sw", "xvfb"],
    )?;
    let offscreen_dir = tempfile::tempdir()?;
    let mut offscreen = gpu_run::offscreen_gpu_command(&std::env::current_exe()?);
    offscreen.args(["--ignored", "--exact", "--nocapture", OFFSCREEN_CHILD]);
    offscreen.env(OUT_VAR, offscreen_dir.path());
    let offscreen_run = offscreen.assert_passes_validation("capture path, offscreen");
    assert!(
        offscreen_run.ledger.contains("\tdisplay=offscreen\t"),
        "the offscreen child ran with no display: {}",
        offscreen_run.ledger
    );
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        keep_captures(
            Path::new(&keep),
            driver,
            &[
                ("window", window_dir.path()),
                ("offscreen", offscreen_dir.path()),
            ],
        )?;
    }
    assert_identical(
        ("window", window_dir.path()),
        ("offscreen", offscreen_dir.path()),
        driver,
    )
}

/// T-2: the window capture through the headless `KWin` equals the one through Xvfb. Only where
/// this process's windowed children use `KWin` (radv on a host with `KWin`); elsewhere it checks
/// that a lavapipe process stays on Xvfb.
#[test]
fn a_kwin_capture_equals_its_xvfb_capture() -> TestResult {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let display = gpu_run::selected_display();
    let driver = gpu_run::selected_driver().driver;
    if display.server != gpu_run::DisplayServer::Kwin {
        if driver == gpu_run::GpuDriver::Lavapipe {
            assert_eq!(display.server, gpu_run::DisplayServer::Xvfb, "{display}");
        }
        eprintln!("a_kwin_capture_equals_its_xvfb_capture: no KWin run, because the {display}");
        return Ok(());
    }
    let (xvfb_dir, _) = window_child(
        gpu_run::DisplayRequest::Xvfb,
        "capture path, window on Xvfb",
        &["xvfb-sw"],
    )?;
    let (kwin_dir, _) = window_child(
        gpu_run::DisplayRequest::Kwin,
        "capture path, window on KWin",
        &["kwin"],
    )?;
    if let Some(keep) = std::env::var_os(KEEP_VAR) {
        keep_captures(
            Path::new(&keep),
            driver.name(),
            &[("xvfb", xvfb_dir.path()), ("kwin", kwin_dir.path())],
        )?;
    }
    assert_identical(
        ("xvfb", xvfb_dir.path()),
        ("kwin", kwin_dir.path()),
        driver.name(),
    )
}
