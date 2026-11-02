//! Native presentation invariants for the locked Bevy/wgpu renderer.
//!
//! Every acquired surface receives real attachment work before presentation,
//! including frames whose camera or screenshot-copy pipelines are still pending.
//! Initialization precedes camera rendering and never replaces the final scene.

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
