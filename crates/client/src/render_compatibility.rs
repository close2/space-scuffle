//! Native presentation invariants for the locked Bevy/wgpu renderer.
//!
//! Every acquired surface receives real attachment work before presentation,
//! including frames whose camera or screenshot-copy pipelines are still pending.
//! Initialization precedes camera rendering and never replaces the final scene.
//!
//! It also owns the one rule about old software Vulkan that more than one tool needs (Q0009):
//! [`mesa_release`] reads the Mesa release from `vulkaninfo --summary`, and
//! [`lavapipe_anisotropy_override`] says whether `lavapipe` of that release needs the terrain
//! anisotropy override. The GPU test harness and the showcase tool both call them.

use bevy::prelude::*;
use bevy::render::graph::CameraDriverLabel;
use bevy::render::render_graph::{
    Node, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel,
};
use bevy::render::render_resource::{
    LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
    TextureViewDescriptor,
};
use bevy::render::renderer::RenderContext;
use bevy::render::view::ExtractedWindows;
use bevy::render::RenderApp;
use bevy::render::{Extract, ExtractSchedule, Render, RenderSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Failure observed while joining the actual renderer and its native work.
#[derive(Debug, thiserror::Error)]
pub enum RendererShutdownError {
    /// No real renderer was available to join.
    #[error("renderer missing during orderly shutdown")]
    MissingRenderer,
    /// The renderer stopped before returning its owned world.
    #[error("render thread stopped before returning its world")]
    RenderThreadStopped,
    /// A worker exceeded the diagnostic deadline; resources stay alive until it finishes.
    #[error("GPU pipeline worker exceeded the 30-second shutdown deadline")]
    PipelineDeadlineExceeded,
}

#[derive(Resource, Clone, Default)]
struct ShutdownStatus(Arc<AtomicBool>);

#[derive(Resource, Default)]
struct FinalRenderFrame(bool);

/// Ensures acquired native window surfaces are initialized before camera output.
///
/// Install after Bevy's renderer; applications without a render sub-app need no
/// native surface work. No simulation state or camera settings are changed.
pub struct RenderCompatibilityPlugin;

impl Plugin for RenderCompatibilityPlugin {
    fn build(&self, app: &mut App) {
        let status = ShutdownStatus::default();
        app.insert_resource(status.clone());
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render
            .insert_resource(status)
            .init_resource::<FinalRenderFrame>()
            .add_systems(ExtractSchedule, extract_exit_request)
            .add_systems(Render, finish_final_render.after(RenderSet::PostCleanup));
        let mut graph = render.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(InitializeAcquiredSurfaces, SurfaceInitializationNode);
        graph.add_node_edge(InitializeAcquiredSurfaces, CameraDriverLabel);
    }
}

/// Runs a graphical app and retains its original error exit reason.
///
/// A shutdown deadline diagnostic turns an otherwise successful exit into an
/// error. The plugin must be installed before this call. No exit event is consumed.
pub fn run_render_app(app: &mut App) -> AppExit {
    let Some(status) = app.world().get_resource::<ShutdownStatus>().cloned() else {
        error!("{}", RendererShutdownError::MissingRenderer);
        return AppExit::error();
    };
    let exit = app.run();
    if exit.is_success() && status.0.load(Ordering::Acquire) {
        AppExit::error()
    } else {
        exit
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn extract_exit_request(
    exits: Extract<'_, '_, Res<'_, Events<AppExit>>>,
    mut final_frame: ResMut<'_, FinalRenderFrame>,
) {
    // Observe all exit reasons without taking ownership from Bevy's runner.
    final_frame.0 = !exits.is_empty();
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn finish_final_render(
    final_frame: Res<'_, FinalRenderFrame>,
    status: Res<'_, ShutdownStatus>,
    mut cache: ResMut<'_, bevy::render::render_resource::PipelineCache>,
    device: Res<'_, bevy::render::renderer::RenderDevice>,
) {
    if final_frame.0 && drain_pipeline_workers(&mut cache, &device).is_err() {
        status.0.store(true, Ordering::Release);
    }
}

fn drain_pipeline_workers(
    cache: &mut bevy::render::render_resource::PipelineCache,
    device: &bevy::render::renderer::RenderDevice,
) -> Result<(), RendererShutdownError> {
    use bevy::render::render_resource::{CachedPipelineState, Maintain};
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut expired = false;
    cache.process_queue();
    let workers = cache
        .pipelines()
        .filter(|pipeline| matches!(pipeline.state, CachedPipelineState::Creating(_)))
        .count();
    info!(
        workers,
        "Joining native renderer pipeline workers before shutdown"
    );
    while cache
        .pipelines()
        .any(|pipeline| matches!(pipeline.state, CachedPipelineState::Creating(_)))
    {
        if !expired && Instant::now() >= deadline {
            expired = true;
            error!(
                "{}; retaining renderer resources until native work joins",
                RendererShutdownError::PipelineDeadlineExceeded
            );
        }
        // A native compiler call cannot be force-cancelled safely. The deadline
        // diagnoses failure; it never authorizes destruction underneath a worker.
        // An external process watchdog handles a driver that never returns.
        std::thread::yield_now();
        cache.process_queue();
    }
    device.poll(Maintain::Wait);
    if expired {
        Err(RendererShutdownError::PipelineDeadlineExceeded)
    } else {
        Ok(())
    }
}

/// Joins a manually driven graphical app before normal destruction.
///
/// This consumes its render sub-app; callers must not update the app afterward.
/// Actual pipeline workers and submitted GPU work finish before resources drop.
///
/// # Errors
/// Reports missing/stopped renderer or a diagnosed worker deadline. Even after a
/// deadline, native work is joined safely before this function returns its error.
pub fn finish_rendering(app: &mut App) -> Result<(), RendererShutdownError> {
    use bevy::render::pipelined_rendering::RenderAppChannels;
    use bevy::render::render_resource::PipelineCache;
    use bevy::render::renderer::RenderDevice;
    let mut render = if let Some(mut channels) =
        app.world_mut().remove_resource::<RenderAppChannels>()
    {
        bevy::tasks::block_on(channels.recv()).ok_or(RendererShutdownError::RenderThreadStopped)?
    } else {
        app.remove_sub_app(RenderApp)
            .ok_or(RendererShutdownError::MissingRenderer)?
    };
    let device = render.world().resource::<RenderDevice>().clone();
    let result = drain_pipeline_workers(
        &mut render.world_mut().resource_mut::<PipelineCache>(),
        &device,
    );
    drop(render);
    result
}

/// The first Mesa release whose `lavapipe` finishes hex-tiled terrain with anisotropic filtering.
///
/// Before it, llvmpipe's anisotropic sampler ignores explicit gradients and `maxAnisotropy`; any
/// clamp above 1 selects that path, so a smaller window or a longer deadline does not help
/// (measurements in `docs/plans/2026-11-02-server-gate.md`).
pub const FIRST_BOUNDED_ANISOTROPY_MESA: (u32, u32) = (25, 1);

/// Extracts `(major, minor)` from `vulkaninfo --summary` text with a line such as
/// `driverInfo = Mesa 22.3.6 (LLVM 15.0.6)`, or returns `None` for any other driver.
#[must_use]
pub fn mesa_release(summary: &str) -> Option<(u32, u32)> {
    let line = summary.lines().find(|line| line.contains("driverInfo"))?;
    let (_, version) = line.split_once("Mesa ")?;
    let mut parts = version.split(['.', ' ', '-']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// The value of the terrain anisotropy override (`SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY`) a
/// `lavapipe` of Mesa `release` needs: `"1"` before [`FIRST_BOUNDED_ANISOTROPY_MESA`], otherwise
/// none. An unknown release keeps the production sampler, so an affected driver fails by timeout
/// rather than quietly losing coverage. Physical GPUs never need it.
#[must_use]
pub fn lavapipe_anisotropy_override(release: Option<(u32, u32)>) -> Option<&'static str> {
    release
        .filter(|release| *release < FIRST_BOUNDED_ANISOTROPY_MESA)
        .map(|_| "1")
}

/// Returns `true` when this `vulkaninfo --summary` names a Mesa release that cannot finish a
/// frame of hex-tiled terrain with anisotropic filtering.
#[must_use]
pub fn driver_stalls_on_anisotropic_hex_tiling(summary: &str) -> bool {
    mesa_release(summary).is_some_and(|release| release < FIRST_BOUNDED_ANISOTROPY_MESA)
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct InitializeAcquiredSurfaces;

struct SurfaceInitializationNode;

impl Node for SurfaceInitializationNode {
    fn run(
        &self,
        _graph: &mut RenderGraphContext<'_>,
        context: &mut RenderContext<'_>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let windows = world.resource::<ExtractedWindows>();
        let clear = world.resource::<ClearColor>().to_linear();
        for window in windows.windows.values() {
            let Some(surface) = &window.swap_chain_texture else {
                continue;
            };
            // PERFORMANCE RATIONALE:
            // Why: Screenshot extraction replaces swap_chain_texture_view with
            // an offscreen target, which cannot consume the surface acquisition.
            // What: Create a view of this acquisition's actual native texture.
            // Trade-off: One GPU view per acquired frame; no substitute texture,
            // scratch collection, retained stale view, or device-wide idle wait.
            let view = surface.texture.create_view(&TextureViewDescriptor {
                format: Some(surface.texture.format().add_srgb_suffix()),
                ..default()
            });
            let attachments = [Some(RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(clear.into()),
                    store: StoreOp::Store,
                },
            })];
            context
                .command_encoder()
                .begin_render_pass(&RenderPassDescriptor {
                    label: Some("initialize_actual_acquired_surface"),
                    color_attachments: &attachments,
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesa_release_is_read_from_the_vulkaninfo_summary() {
        let summary = "\tdeviceName         = llvmpipe (LLVM 15.0.6, 256 bits)\n\
                       \tdriverInfo         = Mesa 22.3.6 (LLVM 15.0.6)\n";
        assert_eq!(mesa_release(summary), Some((22, 3)));
        assert_eq!(
            mesa_release("driverInfo = Mesa 25.2.8-0ubuntu0.24.04.4 (LLVM 20.1.2)"),
            Some((25, 2))
        );
        assert_eq!(mesa_release("driverInfo = 535.183.01"), None);
        assert_eq!(mesa_release("no driver line"), None);
    }

    #[test]
    fn only_lavapipe_before_the_sampler_rewrite_lowers_anisotropy() {
        assert_eq!(lavapipe_anisotropy_override(Some((22, 3))), Some("1"));
        assert_eq!(lavapipe_anisotropy_override(Some((25, 0))), Some("1"));
        assert_eq!(lavapipe_anisotropy_override(Some((25, 1))), None);
        assert_eq!(lavapipe_anisotropy_override(Some((26, 0))), None);
        assert_eq!(lavapipe_anisotropy_override(None), None);
        assert!(driver_stalls_on_anisotropic_hex_tiling(
            "driverInfo = Mesa 25.0.7 (LLVM 19)"
        ));
        assert!(!driver_stalls_on_anisotropic_hex_tiling(
            "driverInfo = Mesa 25.1.0 (LLVM 19)"
        ));
        assert!(!driver_stalls_on_anisotropic_hex_tiling(
            "driverInfo = 535.183.01"
        ));
    }
}
