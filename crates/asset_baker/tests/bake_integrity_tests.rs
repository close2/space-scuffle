//! Tests that a baked atlas truthfully represents its sources.
//!
//! The fixture clip's brightness rises with time, so a baked frame's brightness shows which
//! moment of the clip it came from. All output goes to temporary directories. Tests that need
//! a runtime asset bundle build a fake one, so the real `assets/` directory is never written.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::media::{require_action, MediaLoadError};
use asset_baker::{
    bake_custom_video, bake_multi_action_videos, bake_multi_action_videos_with_states,
    ensure_outside_runtime_assets, generate_all_procedural_atlases, generate_procedural_atlas,
    ActionSlice, BakerError, FlipbookAtlasMeta,
};
use sim_types::enums::AnimState;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// The shipped unit catalog; the baker itself loads it from disk at run time.
fn catalog() -> sim_core::UnitCatalog {
    sim_fixtures::shipped_catalogs().units
}

/// The shipped asset bundle, passed to the CLI explicitly.
fn shipped_assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

mod common;

/// Mean red-channel value of one frame of a vertical strip.
fn frame_brightness(strip: &image::RgbaImage, frame_size: u32, frame: u32) -> f64 {
    let top = frame * frame_size;
    let mut total = 0.0;
    for y in top..top + frame_size {
        for x in 0..frame_size {
            total += f64::from(strip.get_pixel(x, y)[0]);
        }
    }
    total / f64::from(frame_size * frame_size)
}

/// Asserts that a slice runs from the dark start of the clip to its bright end, in order.
fn assert_slice_spans_whole_clip(strip: &image::RgbaImage, size: u32, slice: &ActionSlice) {
    let brightness: Vec<f64> = (slice.start_frame..slice.start_frame + slice.frame_count)
        .map(|frame| frame_brightness(strip, size, frame))
        .collect();
    let first = brightness[0];
    let last = brightness[brightness.len() - 1];
    assert!(first < 40.0, "slice must start at the clip start: {first}");
    assert!(
        last > 200.0,
        "slice must reach the clip end: {brightness:?}"
    );
    assert!(
        brightness.windows(2).all(|pair| pair[1] > pair[0] - 3.0),
        "frames must stay in temporal order: {brightness:?}"
    );
}

fn action_keys(meta: &FlipbookAtlasMeta) -> BTreeSet<AnimState> {
    meta.actions.keys().copied().collect()
}

#[test]
fn b6_default_bake_settings_keep_the_whole_of_every_clip() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");
    let clips = [
        (AnimState::Idle, clip.as_path()),
        (AnimState::Walk, clip.as_path()),
        (AnimState::Attack, clip.as_path()),
    ];

    let meta = bake_multi_action_videos_with_states("subject", &clips, dir.path(), 512, 15)
        .expect("bake at CLI defaults");

    assert!(meta.total_frames * 512 <= 16384);
    let strip = image::open(dir.path().join("subject.png"))
        .expect("strip")
        .to_rgba8();
    for slice in meta.actions.values() {
        assert_slice_spans_whole_clip(&strip, 512, slice);
    }
}

#[test]
fn b6_single_clip_bake_keeps_the_whole_clip() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");

    let meta = bake_custom_video(&clip, dir.path(), "subject", 512, 15).expect("bake");

    assert_eq!(meta.total_frames, 32, "16384 / 512");
    let strip = image::open(dir.path().join("subject.png"))
        .expect("strip")
        .to_rgba8();
    assert_slice_spans_whole_clip(&strip, 512, &meta.actions[&AnimState::Idle]);
}

#[test]
fn b6_a_clip_within_budget_keeps_every_frame() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");

    let meta = bake_multi_action_videos_with_states(
        "subject",
        &[(AnimState::Idle, clip.as_path())],
        dir.path(),
        64,
        5,
    )
    .expect("bake");

    assert_eq!(meta.total_frames, 20, "4 seconds at 5 fps");
}

#[test]
fn b7_only_supplied_clips_become_actions() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");
    let clips = [
        (AnimState::Idle, clip.as_path()),
        (AnimState::Attack, clip.as_path()),
    ];

    let meta =
        bake_multi_action_videos_with_states("subject", &clips, dir.path(), 64, 5).expect("bake");

    assert_eq!(
        action_keys(&meta),
        BTreeSet::from([AnimState::Idle, AnimState::Attack])
    );
    let on_disk = FlipbookAtlasMeta::load_from_ron(&dir.path().join("subject.flipbook.ron"))
        .expect("manifest");
    assert_eq!(action_keys(&on_disk), action_keys(&meta));
}

#[test]
fn b7_require_action_detects_an_action_that_was_never_supplied() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");

    let meta = bake_multi_action_videos_with_states(
        "subject",
        &[(AnimState::Idle, clip.as_path())],
        dir.path(),
        64,
        5,
    )
    .expect("bake");

    assert!(require_action(&meta, dir.path(), AnimState::Idle).is_ok());
    for missing in [
        AnimState::Walk,
        AnimState::Attack,
        AnimState::Cast,
        AnimState::Die,
        AnimState::Constructing,
    ] {
        assert!(
            matches!(
                require_action(&meta, dir.path(), missing),
                Err(MediaLoadError::MissingAction { .. })
            ),
            "{missing:?} must be reported missing"
        );
    }
}

#[test]
fn b7_a_single_video_bake_is_labelled_idle_only() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");

    let meta = bake_custom_video(&clip, dir.path(), "subject", 64, 5).expect("bake");

    assert_eq!(action_keys(&meta), BTreeSet::from([AnimState::Idle]));
}

#[test]
fn b7_surplus_clips_are_refused_rather_than_overwriting_idle() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = [clip.as_path(); 5];

    let result = bake_multi_action_videos("subject", &paths, dir.path(), 64, 5);

    assert!(
        matches!(result, Err(BakerError::ValidationFailed { .. })),
        "{result:?}"
    );
}

/// Creates a directory with the marker of a real runtime asset bundle, and a piece of art.
fn fake_runtime_bundle(root: &Path) -> std::path::PathBuf {
    let bundle = root.join("assets");
    std::fs::create_dir_all(bundle.join("catalog")).expect("catalog dir");
    std::fs::create_dir_all(bundle.join("textures")).expect("textures dir");
    std::fs::write(bundle.join("catalog/units.ron"), "marker").expect("marker");
    bundle
}

fn baker() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asset_baker"));
    command.arg("--assets").arg(shipped_assets());
    command
}

#[test]
fn b8_procedural_names_must_be_exact_catalog_slugs() {
    let catalog = catalog();
    let unit = asset_baker::default_unit_archetypes(&catalog)
        .into_iter()
        .next()
        .expect("catalog has units");
    assert_eq!(
        generate_procedural_atlas(&catalog, &unit, None, None)
            .expect("exact slug")
            .meta
            .name,
        unit
    );

    for near_match in [
        "SiegeTank",
        "command-dome",
        "Railgun-Crawler",
        "worker",
        "bot",
        unit.to_uppercase().as_str(),
    ] {
        assert!(
            matches!(
                generate_procedural_atlas(&catalog, near_match, None, None),
                Err(BakerError::UnknownArchetype { .. })
            ),
            "{near_match:?} must not resolve to some other unit's art"
        );
    }
}

#[test]
fn b8_cli_refuses_to_write_into_a_runtime_bundle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bundle = fake_runtime_bundle(dir.path());
    let unit = &asset_baker::default_unit_archetypes(&catalog())[0];

    let output = baker()
        .current_dir(dir.path())
        .args(["procedural", "--size", "8", "--archetype", unit])
        .arg("--output-dir")
        .arg(bundle.join("textures"))
        .output()
        .expect("run baker");

    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_dir(bundle.join("textures"))
            .expect("textures")
            .count(),
        0,
        "nothing may be written"
    );
}

#[test]
fn b8_cli_has_no_default_output_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bundle = fake_runtime_bundle(dir.path());
    let unit = &asset_baker::default_unit_archetypes(&catalog())[0];

    // The working directory holds a fake bundle, in case the old default is ever restored.
    let output = baker()
        .current_dir(dir.path())
        .args(["procedural", "--size", "8", "--archetype", unit])
        .output()
        .expect("run baker");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--output-dir"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_dir(bundle.join("textures"))
            .expect("textures")
            .count(),
        0
    );
}

#[test]
fn b8_cli_writes_fixture_output_outside_a_bundle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output_dir = dir.path().join("fixtures");
    let unit = &asset_baker::default_unit_archetypes(&catalog())[0];

    let output = baker()
        .current_dir(dir.path())
        .args(["procedural", "--size", "8", "--archetype", unit])
        .arg("--output-dir")
        .arg(&output_dir)
        .output()
        .expect("run baker");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_dir.join(format!("{unit}.png")).is_file());
}

#[test]
fn b8_library_refuses_to_write_into_a_runtime_bundle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bundle = fake_runtime_bundle(dir.path());

    let result =
        generate_all_procedural_atlases(&catalog(), &bundle.join("textures/new_subdirectory"));

    assert!(
        matches!(result, Err(BakerError::RuntimePathRefused { .. })),
        "{result:?}"
    );
    assert!(!bundle.join("textures/new_subdirectory").exists());
}

#[test]
fn b8_the_real_asset_bundle_is_recognised() {
    let textures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/textures");

    let result = ensure_outside_runtime_assets(&textures);

    assert!(
        matches!(result, Err(BakerError::RuntimePathRefused { .. })),
        "{result:?}"
    );
}

#[test]
fn b8_parent_components_cannot_disguise_a_bundle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bundle = fake_runtime_bundle(dir.path());
    let disguised = dir.path().join("missing/../assets/textures/new");

    let result = ensure_outside_runtime_assets(&disguised);

    assert!(
        matches!(result, Err(BakerError::RuntimePathRefused { .. })),
        "{result:?}"
    );
    assert!(bundle.join("catalog/units.ron").is_file());
}

#[cfg(unix)]
#[test]
fn b8_a_symbolic_link_cannot_lead_into_a_bundle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bundle = fake_runtime_bundle(dir.path());
    let link = dir.path().join("innocent");
    std::os::unix::fs::symlink(bundle.join("textures"), &link).expect("symlink");

    let result = ensure_outside_runtime_assets(&link.join("new"));

    assert!(
        matches!(result, Err(BakerError::RuntimePathRefused { .. })),
        "{result:?}"
    );
}

#[test]
fn b8_a_directory_merely_named_assets_is_allowed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let not_a_bundle = dir.path().join("assets/textures");

    ensure_outside_runtime_assets(&not_a_bundle).expect("no bundle marker here");
}

// --- Seam repair leaves other states alone (plan `2026-11-04-veo-seamless-loops.md`, WP2) ----

/// The frames the bake kept for a 256 px centre crop before seam repair existed, rebuilt here
/// from `FFmpeg`'s output with the filter and sampling of that bake: the literal crop filter, and
/// `budget` frames spread from the first to the last.
fn frames_as_baked_before_seam_repair(clip: &Path, budget: usize) -> Vec<image::RgbaImage> {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("/usr/bin/ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(clip)
        .args(["-vf", "crop=min(iw\\,ih):min(iw\\,ih),scale=256:256,fps=15"])
        .arg(dir.path().join("frame_%05d.png"))
        .status()
        .expect("ffmpeg runs");
    assert!(status.success());
    let mut paths: Vec<_> = std::fs::read_dir(dir.path())
        .expect("frames")
        .map(|entry| entry.expect("entry").path())
        .collect();
    paths.sort();
    assert!(paths.len() > budget, "the fixture must be sampled");
    (0..budget)
        .map(|i| i * (paths.len() - 1) / (budget - 1))
        .map(|index| image::open(&paths[index]).expect("frame").to_rgba8())
        .collect()
}

fn strip_frames(strip: &image::RgbaImage, slice: &ActionSlice) -> Vec<image::RgbaImage> {
    (slice.start_frame..slice.start_frame + slice.frame_count)
        .map(|frame| image::imageops::crop_imm(strip, 0, frame * 256, 256, 256).to_image())
        .collect()
}

#[test]
fn states_without_a_seam_bake_byte_for_byte_as_before() {
    use asset_baker::ffmpeg_importer::bake_repaired_action_videos;
    use asset_baker::loop_metrics::SeamRepair;
    use asset_baker::{BakeFraming, FrameSpec};

    let clip = common::fixture_clip();
    let states = [
        AnimState::Idle,
        AnimState::Walk,
        AnimState::Attack,
        AnimState::Die,
    ];
    let clips: Vec<(AnimState, &Path)> = states.iter().map(|s| (*s, clip.as_path())).collect();
    let spec = FrameSpec {
        size: 256,
        fps: 15,
        framing: BakeFraming::CenterCrop,
    };
    // 16384 px / 256 px / 4 states: 16 frames each, sampled from the fixture's 60.
    let before = frames_as_baked_before_seam_repair(&clip, 16);
    let seams = std::collections::BTreeMap::from([(
        AnimState::Idle,
        SeamRepair {
            scale_lock: false,
            crossfade_frames: 2,
        },
    )]);
    let out = tempfile::tempdir().expect("tempdir");
    let bake = bake_repaired_action_videos("seam_neighbours", &clips, out.path(), spec, &seams)
        .expect("bake");
    let strip = image::open(out.path().join("seam_neighbours.png"))
        .expect("atlas")
        .to_rgba8();
    for state in [AnimState::Walk, AnimState::Attack, AnimState::Die] {
        let slice = &bake.meta.actions[&state];
        assert_eq!(strip_frames(&strip, slice), before, "{state:?} changed");
        assert!(bake.seam[&state].is_none());
    }
    let idle = &bake.meta.actions[&AnimState::Idle];
    assert_ne!(strip_frames(&strip, idle), before, "Idle is loop-sampled");
    assert!(bake.seam[&AnimState::Idle].is_some());
}
