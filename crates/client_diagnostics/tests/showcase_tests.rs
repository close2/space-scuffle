//! A real showcase take on the test GPU, rendered twice and compared byte for byte.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, WP2 test 1: a smoke storyboard (480×270, four
//! scenes of four frames: a paused title card, a path at 2×, the army followed at 4×, the status
//! card) is rendered by the debug `showcase take` as two children of the GPU harness
//! (`crates/client/tests/support/gpu_run.rs`). Both manifests must list frames 0 to 15 in order with
//! identical ticks, checksums and BLAKE3 hashes; the mezzanine must decode to 16 frames of 480×270,
//! none of them uniform; the manifest must name the adapter the harness saw.
//!
//! # Invariants
//! - The children render through the harness, which takes the shared lock on `radv` (charter §7,
//!   C21); this process builds no render device.
//! - The storyboard is written to a temporary directory, never under `assets/`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions report failures by panicking"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "frame counts and indices in these fixtures are small"
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "../../client/tests/support/gpu_run.rs"]
mod gpu_run;

use client_diagnostics::showcase::recorder::TakeManifest;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Frames of the smoke storyboard: four scenes of four.
const SMOKE_FRAMES: usize = 16;

/// Width and height of the smoke frames.
const SMOKE_SIZE: (u32, u32) = (480, 270);

/// A four-scene storyboard on the shipped showcase scenario.
const SMOKE_STORYBOARD: &str = r#"(
    format_version: 1,
    resolution: (480, 270),
    frame_rate: 25,
    takes: [(
        name: "smoke",
        mode: Skirmish,
        scenario: "scenarios/showcase.ron",
        readiness_timeout_seconds: 240,
        scenes: [
            (name: "title", speed: Paused, length: Frames(4),
             camera: Orbit(center: At((32.0, 32.0)), distance: 40.0, pitch_deg: 60.0, start_deg: 0.0, sweep_deg: 10.0),
             overlay: TitleCard),
            (name: "path", speed: Speed(2), badge: Some("Fast-forward ×2"), length: Frames(4),
             camera: Path(keys: [
                 (frame: 0, focus: (12.0, 14.0), distance: 30.0, pitch_deg: 58.0),
                 (frame: 3, focus: (20.0, 14.0), distance: 26.0, pitch_deg: 52.0),
             ]),
             overlay: Caption(title: "High ground and ramps", body: "Smoke caption.", subsystems: ["Terrain, cliffs and ramps"])),
            (name: "army", speed: Speed(4), badge: Some("Fast-forward ×4"), length: Frames(4),
             camera: FollowArmy(player: 0, distance: 20.0, pitch_deg: 55.0),
             overlay: None),
            (name: "status", speed: Speed(1), length: Frames(4),
             camera: Orbit(center: Current, distance: 30.0, pitch_deg: 60.0, start_deg: 0.0, sweep_deg: 5.0),
             overlay: StatusCard),
        ],
    )],
    video: (crf: 23, max_kbit_per_second: 2500, max_bytes: 26214400),
    teaser: (frame_step: 2, widths: [320], qualities: [60], max_bytes: 4718592,
             cuts: [(scene: "title", first: 0, frames: 4)]),
    poster: (scene: "army", frame: 1),
)"#;

/// Renders the smoke take into `out` through the harness and returns its manifest.
fn render(
    storyboard: &Path,
    out: &Path,
    label: &str,
) -> Result<(TakeManifest, gpu_run::GpuRun), Box<dyn Error>> {
    let mut command = gpu_run::virtual_gpu_command_on_screen(
        Path::new(env!("CARGO_BIN_EXE_showcase")),
        Some("1280x720x24"),
    );
    command
        .allow_for_hex_tiling_driver_defect()
        .arg("take")
        .arg("--storyboard")
        .arg(storyboard)
        .args([
            "--take",
            "smoke",
            "--revision",
            "0123456789abcdef",
            "--revision-date",
            "2026-11-05",
        ])
        .arg("--out")
        .arg(out);
    let run = command.run(label);
    eprintln!("{label} took {:.1} s", run.run_time.as_secs_f64());
    let manifest = TakeManifest::load(&TakeManifest::path(out, "smoke"))?;
    Ok((manifest, run))
}

/// Decodes the mezzanine into raw RGB frames with the system `ffmpeg`.
fn decode(path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()?;
    if !output.status.success() {
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(output.stdout)
}

#[test]
fn showcase_smoke_take_is_byte_reproducible() -> Result<(), Box<dyn Error>> {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let work = tempfile::tempdir()?;
    let storyboard = work.path().join("smoke.ron");
    std::fs::write(&storyboard, SMOKE_STORYBOARD)?;
    let outputs: Vec<PathBuf> = ["first", "second"]
        .iter()
        .map(|name| work.path().join(name))
        .collect();
    let (first, run) = render(&storyboard, &outputs[0], "showcase smoke take, first")?;
    let (second, _) = render(&storyboard, &outputs[1], "showcase smoke take, second")?;
    for manifest in [&first, &second] {
        assert!(manifest.complete, "{:?}", manifest.failure);
        assert_eq!(manifest.frames.len(), SMOKE_FRAMES);
        for (expected, frame) in manifest.frames.iter().enumerate() {
            assert_eq!(frame.index, expected as u64);
        }
        assert_eq!(
            manifest
                .scenes
                .iter()
                .map(|scene| scene.frames)
                .collect::<Vec<_>>(),
            vec![4, 4, 4, 4]
        );
    }
    let summary = |manifest: &TakeManifest| {
        manifest
            .frames
            .iter()
            .map(|frame| (frame.tick, frame.checksum, frame.blake3.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        summary(&first),
        summary(&second),
        "two takes of one commit differ"
    );
    // Paused title, then 2 ticks a frame, then 4: the battle advanced exactly as the scenes asked.
    let ticks: Vec<u32> = first.frames.iter().map(|frame| frame.tick).collect();
    assert_eq!(&ticks[..4], &[0, 0, 0, 0]);
    assert_eq!(ticks[7] - ticks[3], 8);
    assert_eq!(ticks[11] - ticks[7], 16);
    if let Some(adapter) = &run.adapter {
        assert_eq!(first.adapter.name, adapter.name);
    }
    assert_ne!(first.adapter.name, "");
    let pixels = decode(&outputs[0].join(&first.mezzanine))?;
    let frame_bytes = SMOKE_SIZE.0 as usize * SMOKE_SIZE.1 as usize * 3;
    assert_eq!(
        pixels.len(),
        frame_bytes * SMOKE_FRAMES,
        "the mezzanine decodes to 16 frames of 480x270"
    );
    for (index, frame) in pixels.chunks(frame_bytes).enumerate() {
        let uniform = frame.chunks(3).all(|pixel| pixel == &frame[..3]);
        assert!(!uniform, "frame {index} is a single colour");
        assert_eq!(
            blake3::hash(frame).to_hex().to_string(),
            first.frames[index].blake3,
            "frame {index} of the mezzanine is not the frame read back"
        );
    }
    Ok(())
}
