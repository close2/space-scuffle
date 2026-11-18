//! Offscreen rendering for programs that read only their own pictures: a canvas image the
//! camera renders into, the plugin settings of an app with no window, and the one place that
//! says what a screenshot captures.
//!
//! # Responsibility
//! - [`OffscreenCanvas`]: an [`Image`] of [`CANVAS_FORMAT`] that a camera renders into and a
//!   [`Screenshot`] copies from, with the usages both need.
//! - [`windowless_plugins`]: turns the presentation plugin group into one with no primary
//!   window, no winit event loop and a [`ScheduleRunnerPlugin`] loop, so the app needs no
//!   display at all.
//! - [`CaptureTarget`]: what a program's camera renders into and its screenshots copy from,
//!   the primary window or a canvas, as a resource the program's systems read.
//!
//! # How it fits
//! The effect lab ([`crate::effects::lab`]) and the GPU test children that build their own apps
//! use these instead of a hidden window. Under a test display a window costs every frame a
//! present: on the RX 580, radv's software present under Xvfb copies each 1280×720 frame
//! through the CPU (about 105 ms; `docs/evidence/2026-11-17-gpu-test-readback.md`). An
//! offscreen frame is never presented, and a screenshot is one GPU copy into a host-cached
//! buffer on the frame that asks for it (Bevy's own capture path; plan
//! `docs/plans/2026-11-17-gpu-test-readback.md`, decisions D-1 to D-3).
//!
//! # Invariants
//! - The canvas has the format the swapchain has under the test display today
//!   (`Bgra8UnormSrgb`, measured in the WP1 evidence), so the render graph writes the same
//!   output texture format and the pictures are bit-identical to a window capture.
//! - A windowless app has no `PrimaryWindow` and no `WinitPlugin`; it stops only on
//!   [`AppExit`], which the program sends.
//! - A canvas is one persistent texture. A frame in which no camera renders leaves the previous
//!   frame in it: nothing clears it between frames. A window's swapchain hands out a texture
//!   per frame, and the clear on a surface's initialisation covers only those textures.
//! - That cannot leak an old frame into a capture. In Bevy 0.15.3 a [`Screenshot`] redirects the
//!   output of the frame it is taken on into a new texture of its own and copies that texture
//!   (`bevy_render::view::window::screenshot`, the `NormalizedRenderTarget::Image` branches), so
//!   it holds what that frame's camera wrote, never what the canvas held before; a new wgpu
//!   texture starts zeroed, so a frame with no camera would capture black, not a stale frame.
//!   The programs' cameras render every frame, and they ask for a capture only after their
//!   settle frames (the lab driver's `settled`), so each capture shows the settled scene.

use bevy::app::{PluginGroupBuilder, ScheduleRunnerPlugin};
use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::screenshot::Screenshot;
use bevy::window::{ExitCondition, WindowRef};
use std::time::Duration;

/// Format of every canvas: the surface format Bevy configures for a window on the test
/// display, on radv and on lavapipe (readback evidence, M5).
pub const CANVAS_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

/// Colour a canvas holds before the first frame: opaque black, in [`CANVAS_FORMAT`]'s byte
/// order (blue, green, red, alpha).
const CANVAS_FILL: [u8; 4] = [0, 0, 0, 255];

/// Usages of a canvas: rendered into (`RENDER_ATTACHMENT`), copied out by a screenshot
/// (`COPY_SRC`), and the defaults an [`Image`] asset has (`TEXTURE_BINDING`, `COPY_DST`).
pub const CANVAS_USAGES: TextureUsages = TextureUsages::RENDER_ATTACHMENT
    .union(TextureUsages::COPY_SRC)
    .union(TextureUsages::TEXTURE_BINDING)
    .union(TextureUsages::COPY_DST);

/// A canvas that is not created.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OffscreenError {
    /// The app has no `Assets<Image>`: the presentation plugins are not added yet.
    #[error("the app has no image assets; add the presentation plugins before the canvas")]
    NoImageAssets,
    /// A side of the canvas is zero.
    #[error("a canvas of {width}x{height} pixels has no area")]
    EmptyCanvas {
        /// Width, pixels.
        width: u32,
        /// Height, pixels.
        height: u32,
    },
}

/// The image a windowless program's camera renders into.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct OffscreenCanvas {
    /// The canvas image asset.
    pub image: Handle<Image>,
    /// Its size, physical pixels (the scale factor of an image target is 1).
    pub size: UVec2,
}

impl OffscreenCanvas {
    /// The canvas image of `size` physical pixels: [`CANVAS_FORMAT`], [`CANVAS_USAGES`], filled
    /// with opaque black, kept in both the main and the render world.
    ///
    /// # Errors
    /// [`OffscreenError::EmptyCanvas`] when a side is zero.
    pub fn image(size: UVec2) -> Result<Image, OffscreenError> {
        if size.x == 0 || size.y == 0 {
            return Err(OffscreenError::EmptyCanvas {
                width: size.x,
                height: size.y,
            });
        }
        let mut image = Image::new_fill(
            Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &CANVAS_FILL,
            CANVAS_FORMAT,
            RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage = CANVAS_USAGES;
        Ok(image)
    }

    /// Adds a canvas of `size` to `images`.
    ///
    /// # Errors
    /// As [`Self::image`].
    pub fn create(images: &mut Assets<Image>, size: UVec2) -> Result<Self, OffscreenError> {
        Ok(Self {
            image: images.add(Self::image(size)?),
            size,
        })
    }

    /// Creates a canvas of `size` in `app` and inserts it with [`CaptureTarget::Image`] of it,
    /// so the program's camera and screenshots use it. Call it after the presentation plugins.
    ///
    /// # Errors
    /// [`OffscreenError::NoImageAssets`] when the app has no image assets, and as
    /// [`Self::image`].
    pub fn install(app: &mut App, size: UVec2) -> Result<Self, OffscreenError> {
        let mut images = app
            .world_mut()
            .get_resource_mut::<Assets<Image>>()
            .ok_or(OffscreenError::NoImageAssets)?;
        let canvas = Self::create(&mut images, size)?;
        app.insert_resource(canvas.clone())
            .insert_resource(CaptureTarget::Image(canvas.image.clone()));
        Ok(canvas)
    }
}

/// What a program's camera renders into and its screenshots copy from.
#[derive(Resource, Debug, Clone, PartialEq)]
pub enum CaptureTarget {
    /// The primary window: a camera's default target.
    PrimaryWindow,
    /// An [`OffscreenCanvas`] image.
    Image(Handle<Image>),
}

impl CaptureTarget {
    /// A screenshot of this target, captured on the frame after it is spawned.
    #[must_use]
    pub fn screenshot(&self) -> Screenshot {
        match self {
            Self::PrimaryWindow => Screenshot::primary_window(),
            Self::Image(image) => Screenshot::image(image.clone()),
        }
    }

    /// The render target of the program's camera.
    #[must_use]
    pub fn camera_target(&self) -> RenderTarget {
        match self {
            Self::PrimaryWindow => RenderTarget::Window(WindowRef::Primary),
            Self::Image(image) => RenderTarget::Image(image.clone()),
        }
    }
}

/// The presentation plugin group `group` (such as `GamePresentationPlugins.build()` with its
/// settings) without a window: no primary window, an app that does not exit when windows
/// close, no winit, and Bevy's schedule runner looping with no wait between frames.
///
/// Without winit nothing runs the frames: Bevy's default runner would update the app once and
/// return, so the loop is required. The app then ends only on [`AppExit`].
///
/// # Panics
/// Panics, in Bevy's plugin group builder, when `group` has no `WindowPlugin` or no
/// `WinitPlugin`; Bevy's default plugins and `GamePresentationPlugins` have both.
#[must_use]
pub fn windowless_plugins(group: PluginGroupBuilder) -> PluginGroupBuilder {
    group
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            close_when_requested: false,
        })
        .disable::<bevy::winit::WinitPlugin<bevy::winit::WakeUp>>()
        .add(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::settings::{RenderCreation, WgpuSettings};
    use bevy::render::RenderPlugin;
    use bevy::window::PrimaryWindow;

    /// T-7: the canvas has the four usages, the chosen format, the size and opaque black.
    #[test]
    fn the_canvas_has_the_swapchain_format_and_the_capture_usages(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let image = OffscreenCanvas::image(UVec2::new(4, 3))?;
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Bgra8UnormSrgb
        );
        for usage in [
            TextureUsages::RENDER_ATTACHMENT,
            TextureUsages::COPY_SRC,
            TextureUsages::TEXTURE_BINDING,
            TextureUsages::COPY_DST,
        ] {
            assert!(image.texture_descriptor.usage.contains(usage), "{usage:?}");
        }
        assert_eq!(image.texture_descriptor.usage, CANVAS_USAGES);
        assert_eq!(image.size(), UVec2::new(4, 3));
        assert_eq!(image.texture_descriptor.dimension, TextureDimension::D2);
        assert_eq!(image.data.len(), 4 * 3 * 4);
        assert!(image
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == CANVAS_FILL));
        assert_eq!(image.asset_usage, RenderAssetUsages::default());
        Ok(())
    }

    /// A canvas without area is refused, and a canvas needs the image assets.
    #[test]
    fn an_empty_canvas_or_an_app_without_images_is_an_error() {
        assert_eq!(
            OffscreenCanvas::image(UVec2::new(0, 720)).err(),
            Some(OffscreenError::EmptyCanvas {
                width: 0,
                height: 720
            })
        );
        assert_eq!(
            OffscreenCanvas::install(&mut App::new(), UVec2::new(8, 8)),
            Err(OffscreenError::NoImageAssets)
        );
    }

    /// The canvas installs itself and the capture target of its image.
    #[test]
    fn an_installed_canvas_is_the_capture_target() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>();
        let canvas = OffscreenCanvas::install(&mut app, UVec2::new(16, 9))?;
        assert_eq!(app.world().resource::<OffscreenCanvas>(), &canvas);
        assert_eq!(
            app.world().resource::<CaptureTarget>(),
            &CaptureTarget::Image(canvas.image.clone())
        );
        let stored = app
            .world()
            .resource::<Assets<Image>>()
            .get(&canvas.image)
            .ok_or("the canvas image is stored")?;
        assert_eq!(stored.size(), UVec2::new(16, 9));
        Ok(())
    }

    /// Whether `target` is the image `image`.
    fn is_image(target: &RenderTarget, image: &Handle<Image>) -> bool {
        matches!(target, RenderTarget::Image(handle) if handle == image)
    }

    /// Whether `target` is the primary window.
    fn is_primary_window(target: &RenderTarget) -> bool {
        matches!(target, RenderTarget::Window(WindowRef::Primary))
    }

    /// T-7: `CaptureTarget::Image` yields `Screenshot::image` of its handle, and the window
    /// target the primary window (a camera's default target), for the screenshot and the camera
    /// alike.
    #[test]
    fn a_capture_target_names_the_screenshot_and_camera_target() {
        let image = Handle::<Image>::weak_from_u128(0x5ca1_ab1e);
        let canvas = CaptureTarget::Image(image.clone());
        assert!(is_image(&canvas.screenshot().0, &image));
        assert!(is_image(&canvas.camera_target(), &image));
        let window = CaptureTarget::PrimaryWindow;
        assert!(is_primary_window(&window.screenshot().0));
        assert!(is_primary_window(&window.camera_target()));
        assert!(is_primary_window(&Camera::default().target));
    }

    /// T-7: the windowless group adds no primary window and no winit, and loops the schedule.
    /// The renderer's backends are off, so no render device is built here (charter C21).
    #[test]
    fn the_windowless_plugins_leave_no_window_and_no_winit() {
        let mut app = App::new();
        app.add_plugins(windowless_plugins(
            crate::presentation::GamePresentationPlugins
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings {
                        backends: None,
                        ..default()
                    }),
                    ..default()
                })
                .disable::<bevy::log::LogPlugin>(),
        ));
        app.finish();
        app.cleanup();
        assert!(!app.is_plugin_added::<bevy::winit::WinitPlugin<bevy::winit::WakeUp>>());
        assert!(app.is_plugin_added::<ScheduleRunnerPlugin>());
        assert!(app.is_plugin_added::<WindowPlugin>());
        app.update();
        let windows = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .iter(app.world())
            .count();
        assert_eq!(windows, 0, "a windowless app has no primary window");
        let all_windows = app.world_mut().query::<&Window>().iter(app.world()).count();
        assert_eq!(all_windows, 0, "a windowless app has no window at all");
    }
}
