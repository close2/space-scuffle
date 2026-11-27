//! The production client on a real renderer, drawing into an offscreen canvas, for GPU test
//! children that capture a battle.
//!
//! # Responsibility
//! - [`boot_on_gpu`]: boots the production client on a scenario as `support/battle_harness.rs`
//!   does, but with a real renderer on the adapter the GPU harness (`support/gpu_run.rs`) selected
//!   for the child, the RTS camera drawing into an [`OffscreenCanvas`] and the smog volume off
//!   (`LabState::opt_smog_volume`, the production toggle). The window has no surface (there is no
//!   winit), so the camera draws only into the canvas.
//! - [`ready`], [`run_ticks`] and [`capture`]: wait for the battle's media, advance the paused
//!   battle, and capture the canvas with the clock stopped.
//!
//! # How tests use it
//! Include it with `#[path = "support/gpu_battle.rs"]` next to `battle_harness.rs` and
//! `gpu_run.rs`; only a child that `gpu_run` launches may call [`boot_on_gpu`], which checks it
//! (charter §7). It follows `render_pool_gpu_tests.rs`, which keeps its own copy of the same boot.

#![allow(clippy::expect_used, clippy::panic, clippy::cast_precision_loss)]

use std::path::Path;
use std::time::Duration;

use bevy::prelude::*;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::render::view::screenshot::save_to_disk;
use bevy::render::RenderPlugin;
use bevy::time::TimeUpdateStrategy;
use client::offscreen::{CaptureTarget, OffscreenCanvas};
use client::scene_controller::RenderEntityMap;
use client::{ClientMode, RtsCamera};

use crate::battle_harness::{self, Harness};
use crate::gpu_run;

/// Frames the battle gets to load its media.
const READY_FRAMES: usize = 600;

/// Frames a capture may take to reach the disk.
const CAPTURE_FRAMES: usize = 120;

/// Panics unless this process is a child that `gpu_run` launched: the harness gives every child
/// the Vulkan manifest of the driver it selected, and holds the GPU lock while the child runs, so
/// no render device is built outside the harness (charter §7).
fn assert_launched_by_gpu_run() {
    let selection = gpu_run::selected_driver();
    let manifest = std::env::var_os("VK_ICD_FILENAMES");
    assert_eq!(
        manifest.as_deref(),
        Some(selection.manifest.as_os_str()),
        "a GPU battle boots only in a child that gpu_run launched ({selection})"
    );
}

/// Boots the production client on `scenario` (a path beneath the assets root) with a real
/// renderer, drawing into an offscreen canvas of `size` physical pixels, the smog volume off,
/// paused. Only a child that `gpu_run` launched may call it.
pub fn boot_on_gpu(scenario: &str, size: UVec2) -> Harness {
    assert_launched_by_gpu_run();
    let root =
        client::ResolvedAssetsRoot::from_directory(&battle_harness::assets()).expect("assets root");
    let definition =
        client::runtime_battle::BattleDefinition::load(root.as_path(), scenario.as_ref())
            .expect("battle scenario");
    let mut app = App::new();
    app.insert_resource(client::CatalogResource(definition.catalogs.units.clone()))
        .insert_resource(definition.palette.clone())
        .insert_resource(definition)
        .insert_resource(root.clone())
        .add_plugins(
            client::presentation::GamePresentationPlugins
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings::default()),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: root.bevy_file_path().expect("asset path").into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        resolution: (size.x as f32, size.y as f32).into(),
                        ..default()
                    }),
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>(),
        )
        .add_plugins(bevy_egui::EguiPlugin)
        .add_plugins(client::ClientAppPlugin {
            mode: ClientMode::LiveBattle,
            max_frames: None,
            screenshot_out: None,
        })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            40,
        )));
    app.finish();
    app.cleanup();
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = true;
    let mut harness = Harness { app };
    harness.step(4);
    let canvas = OffscreenCanvas::install(&mut harness.app, size).expect("a canvas");
    let world = harness.app.world_mut();
    let mut cameras = world.query_filtered::<&mut Camera, With<RtsCamera>>();
    for mut camera in cameras.iter_mut(world) {
        camera.target = CaptureTarget::Image(canvas.image.clone()).camera_target();
    }
    world.resource_mut::<client::LabState>().opt_smog_volume = false;
    harness
}

/// Steps until the battle is ready, with its media loaded and its first snapshot drawn.
pub fn ready(harness: &mut Harness) {
    for _ in 0..READY_FRAMES {
        let world = harness.app.world();
        let images = world.resource::<Assets<Image>>();
        let registry = world.resource::<client::FlipbookRegistry>();
        let loaded = !registry.textures_by_id.is_empty()
            && registry
                .textures_by_id
                .values()
                .all(|image| images.contains(image));
        if loaded && !world.resource::<RenderEntityMap>().is_empty() {
            return;
        }
        harness.step(1);
    }
    panic!("the battle never became ready");
}

/// Runs `ticks` ticks, one a frame, and pauses.
pub fn run_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    harness.resume();
    while harness.world().current_tick < goal {
        harness.step(1);
    }
    harness.pause();
}

/// Captures the canvas into `directory/name.png` with the clock stopped: virtual time is paused
/// while the capture travels, so every frame until it lands draws the same scene.
pub fn capture(harness: &mut Harness, directory: &Path, name: &str) {
    let path = directory.join(format!("{name}.png"));
    harness
        .app
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .pause();
    let target = harness.app.world().resource::<CaptureTarget>().clone();
    harness
        .app
        .world_mut()
        .spawn(target.screenshot())
        .observe(save_to_disk(path.clone()));
    for _ in 0..CAPTURE_FRAMES {
        if path.is_file() {
            break;
        }
        harness.step(1);
    }
    assert!(path.is_file(), "the capture {name} reached the disk");
    harness
        .app
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .unpause();
}
