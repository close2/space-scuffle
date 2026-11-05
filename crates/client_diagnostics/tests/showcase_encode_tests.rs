//! Assembling the showcase site from a take, with the system `ffmpeg`, through the public API.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, WP3 tests 1 to 3 (test 4, the launch of take
//! children on `radv` and `lavapipe`, is a unit test of `showcase::render` with an injected host):
//! - `assemble` writes every file within budget: H.264 `yuv420p` at 25 fps with every frame, an
//!   animated, looping WebP with the teaser's frame count, a poster, and a provenance record whose
//!   SHA-256 digests match the files;
//! - the size ladder steps down in order and then fails loudly with every size it tried;
//! - the player page template has every placeholder filled, names the four files, escapes text.
//!
//! # Invariants
//! - The mezzanine here is a temporary fixture made with ffmpeg's `testsrc2`, never under
//!   `assets/` (charter §1.2); it stands in for engine frames only to test cutting and encoding.
//! - The test needs `ffmpeg` and fails without it, like `asset_baker`'s ffmpeg tests.

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

use client_diagnostics::showcase::encode::{
    self, PAGE_FILE, POSTER_FILE, PROVENANCE_FILE, TEASER_FILE, VIDEO_FILE,
};
use client_diagnostics::showcase::recorder::{
    AdapterRecord, FrameRecord, SceneRecord, TakeManifest, MANIFEST_VERSION,
};
use client_diagnostics::showcase::render::{assemble_with, AssembleOptions};
use client_diagnostics::showcase::site::{self, PageFacts, Provenance};
use client_diagnostics::showcase::storyboard::{Cut, Length, Storyboard};
use client_diagnostics::showcase::{ShowcaseError, ValidatedStoryboard, DEFAULT_STORYBOARD};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

const FRAMES: u64 = 60;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The shipped storyboard, reduced to one take of three 20-frame scenes at 320×180.
fn fixture_storyboard() -> Result<ValidatedStoryboard, Box<dyn Error>> {
    let mut validated = ValidatedStoryboard::load(
        &repository().join("assets"),
        &repository(),
        Path::new(DEFAULT_STORYBOARD),
    )?;
    let storyboard: &mut Storyboard = &mut validated.storyboard;
    storyboard.resolution = (320, 180);
    storyboard.takes.truncate(1);
    let take = &mut storyboard.takes[0];
    take.name = "fixture".into();
    take.scenes.truncate(3);
    for scene in &mut take.scenes {
        scene.length = Length::Frames(20);
    }
    let names: Vec<String> = take.scenes.iter().map(|scene| scene.name.clone()).collect();
    storyboard.teaser.widths = vec![160];
    storyboard.teaser.qualities = vec![60];
    storyboard.teaser.frame_step = 2;
    storyboard.teaser.cuts = vec![
        Cut {
            scene: names[0].clone(),
            first: 0,
            frames: 10,
            from_end: false,
        },
        Cut {
            scene: names[1].clone(),
            first: 0,
            frames: 10,
            from_end: true,
        },
        Cut {
            scene: names[2].clone(),
            first: 5,
            frames: 7,
            from_end: false,
        },
    ];
    storyboard.poster.scene.clone_from(&names[2]);
    storyboard.poster.frame = 3;
    Ok(validated)
}

/// A 60-frame FFV1 mezzanine of `testsrc2` and its manifest, in `directory`.
fn fixture_take(validated: &ValidatedStoryboard, directory: &Path) -> Result<(), Box<dyn Error>> {
    let take = &validated.storyboard.takes[0];
    let mezzanine = directory.join(format!("{}.mkv", take.name));
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg(format!(
            "testsrc2=size=320x180:rate=25:duration={}",
            FRAMES as f64 / 25.0
        ))
        .args([
            "-pix_fmt", "rgb24", "-c:v", "ffv1", "-level", "3", "-g", "1",
        ])
        .arg(&mezzanine)
        .status()?;
    assert!(status.success(), "ffmpeg is required for this test");
    let manifest = TakeManifest {
        format_version: MANIFEST_VERSION,
        complete: true,
        failure: None,
        take: take.name.clone(),
        storyboard: validated.path.clone(),
        scenario: take.scenario.clone(),
        mezzanine: format!("{}.mkv", take.name),
        resolution: (320, 180),
        frame_rate: 25,
        adapter: AdapterRecord {
            name: "fixture <adapter>".into(),
            driver: "none".into(),
            driver_info: "Mesa 25.0.7".into(),
            backend: "Vulkan".into(),
        },
        anisotropy_override: Some("1".into()),
        revision: "0123456".into(),
        revision_date: "2026-11-05".into(),
        readiness_seconds: 1.0,
        render_seconds: 2.0,
        scenes: take
            .scenes
            .iter()
            .enumerate()
            .map(|(index, scene)| SceneRecord {
                name: scene.name.clone(),
                first_frame: index as u64 * 20,
                frames: 20,
                start_tick: index as u32 * 20,
            })
            .collect(),
        frames: (0..FRAMES)
            .map(|index| FrameRecord {
                index,
                tick: index as u32,
                checksum: index,
                blake3: String::new(),
                wall_ms: 10.0,
            })
            .collect(),
    };
    manifest.save(&TakeManifest::path(directory, &take.name))?;
    Ok(())
}

/// `ffprobe` of the first video stream: codec, pixel format, frame rate and frame count.
fn probe(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
        ])
        .arg("stream=codec_name,pix_fmt,r_frame_rate,nb_read_frames,width")
        .args(["-of", "default=noprint_wrappers=1"])
        .arg(path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

/// Frames of an animated WebP: its `ANMF` chunks; and whether it loops forever.
fn webp_frames(bytes: &[u8]) -> (usize, bool) {
    let frames = bytes.windows(4).filter(|window| window == b"ANMF").count();
    let loops = bytes
        .windows(4)
        .position(|window| window == b"ANIM")
        .is_some_and(|at| bytes.len() > at + 14 && bytes[at + 12..at + 14] == [0, 0]);
    (frames, loops)
}

#[test]
fn assemble_writes_every_file_within_budget() -> Result<(), Box<dyn Error>> {
    let validated = fixture_storyboard()?;
    let work = tempfile::tempdir()?;
    let takes = work.path().join("takes");
    std::fs::create_dir(&takes)?;
    fixture_take(&validated, &takes)?;
    let out = work.path().join("site");
    assemble_with(
        &validated,
        &AssembleOptions {
            storyboard: validated.path.clone(),
            takes,
            revision: "0123456789".into(),
            revision_date: "2026-11-05".into(),
            run_url: Some("https://example.invalid/run/1".into()),
            out: out.clone(),
        },
    )?;
    for name in [
        VIDEO_FILE,
        TEASER_FILE,
        POSTER_FILE,
        PROVENANCE_FILE,
        PAGE_FILE,
    ] {
        assert!(out.join(name).is_file(), "{name}");
    }
    let video = probe(&out.join(VIDEO_FILE))?;
    for line in [
        "codec_name=h264",
        "pix_fmt=yuv420p",
        "r_frame_rate=25/1",
        "width=320",
    ] {
        assert!(
            video.iter().any(|entry| entry == line),
            "{line} in {video:?}"
        );
    }
    assert!(
        video
            .iter()
            .any(|entry| entry == &format!("nb_read_frames={FRAMES}")),
        "{video:?}"
    );
    // Cuts of 10 + 10 + 7 frames, every second frame kept: 5 + 5 + 4 = ceil(27 / 2) = 14.
    let teaser = std::fs::read(out.join(TEASER_FILE))?;
    assert_eq!(&teaser[..4], b"RIFF");
    assert_eq!(webp_frames(&teaser), (14, true));
    assert!(teaser.len() as u64 <= validated.storyboard.teaser.max_bytes);
    let video_size = std::fs::metadata(out.join(VIDEO_FILE))?.len();
    assert!(video_size <= validated.storyboard.video.max_bytes);
    let provenance: Provenance =
        serde_json::from_str(&std::fs::read_to_string(out.join(PROVENANCE_FILE))?)?;
    assert_eq!(provenance.files.len(), 3);
    for file in &provenance.files {
        let record = site::FileRecord::of(&out.join(&file.name))?;
        assert_eq!(&record, file);
    }
    assert_eq!(provenance.teaser, (160, 60));
    assert_eq!(provenance.takes[0].mesa_release.as_deref(), Some("25.0"));
    assert_eq!(
        provenance.takes[0].anisotropy_override.as_deref(),
        Some("1")
    );
    let page = std::fs::read_to_string(out.join(PAGE_FILE))?;
    assert!(page.contains("fixture &lt;adapter&gt;"));
    // A second assembly into the same directory is refused rather than overwriting it.
    let again = assemble_with(
        &validated,
        &AssembleOptions {
            storyboard: validated.path.clone(),
            takes: work.path().join("takes"),
            revision: "0".into(),
            revision_date: "2026-11-05".into(),
            run_url: None,
            out,
        },
    );
    assert!(matches!(again, Err(ShowcaseError::Render(_))), "{again:?}");
    Ok(())
}

#[test]
fn size_ladder_steps_down_then_fails_loudly() {
    let mut tried = Vec::new();
    let result = encode::ladder(
        "teaser.webp",
        &[800, 640],
        &[70, 60],
        10,
        |width, quality| {
            tried.push((width, quality));
            Ok(u64::from(width + quality))
        },
    );
    assert_eq!(tried, vec![(800, 70), (800, 60), (640, 70), (640, 60)]);
    match result {
        Err(ShowcaseError::OverBudget {
            output,
            budget,
            attempts,
        }) => {
            assert_eq!((output.as_str(), budget), ("teaser.webp", 10));
            assert_eq!(attempts.len(), 4);
            assert!(
                attempts[0].contains("800 px at quality 70: 870 bytes"),
                "{attempts:?}"
            );
            assert!(attempts[3].contains("640 px at quality 60"), "{attempts:?}");
        }
        other => panic!("{other:?}"),
    }
    let mut calls = 0;
    let kept = encode::ladder(
        "teaser.webp",
        &[800, 640],
        &[70, 60],
        1_000,
        |width, quality| {
            calls += 1;
            Ok(u64::from(width + quality))
        },
    );
    let (attempt, attempts) = kept.expect("the first rung fits");
    assert_eq!(
        (attempt.width, attempt.quality, calls, attempts.len()),
        (800, 70, 1, 1)
    );
}

#[test]
fn site_template_replaces_every_placeholder() -> Result<(), Box<dyn Error>> {
    let page = site::render_page(
        site::TEMPLATE,
        &PageFacts {
            pitch: "A <game> & \"more\".".into(),
            revision: "abc1234".into(),
            revision_date: "2026-11-05".into(),
            adapter: "llvmpipe (LLVM 19.1.7, 256 bits)".into(),
            anisotropy: None,
            run_url: Some("https://example.invalid/run?a=1&b=2".into()),
            scenes: vec![SceneRecord {
                name: "title".into(),
                first_frame: 0,
                frames: 125,
                start_tick: 0,
            }],
        },
    )?;
    assert!(!page.contains("{{"), "a placeholder is left");
    for file in [VIDEO_FILE, TEASER_FILE, POSTER_FILE, PROVENANCE_FILE] {
        assert!(page.contains(file), "{file}");
    }
    assert!(page.contains("A &lt;game&gt; &amp; &quot;more&quot;."));
    assert!(page.contains("a=1&amp;b=2"));
    assert!(page.contains("<td>title</td><td>125</td>"));
    assert!(!page.contains("<script"));
    Ok(())
}
