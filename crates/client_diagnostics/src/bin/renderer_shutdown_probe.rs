//! Exercises early native shutdown through the actual production game renderer.
//!
//! The probe exits while real shaders compile, preserving the requested success
//! or error reason. Vulkan validation and the external process watchdog determine
//! whether native resources survive until all compilation and submission work ends.

use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache};
use bevy::render::{Render, RenderApp, RenderSet};
use clap::Parser;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Parser, Resource)]
struct Options {
    /// Exercise preservation of an existing nonzero `AppExit` reason.
    #[arg(long)]
    error_exit: bool,
}

#[derive(Resource, Clone, Default)]
struct CompileObserved(Arc<AtomicBool>);

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn inspect_compilation(cache: Res<'_, PipelineCache>, observed: Res<'_, CompileObserved>) {
    if cache
        .pipelines()
        .any(|pipeline| matches!(pipeline.state, CachedPipelineState::Creating(_)))
    {
        observed.0.store(true, Ordering::Release);
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn request_early_exit(
    observed: Res<'_, CompileObserved>,
    options: Res<'_, Options>,
    mut exit: EventWriter<'_, AppExit>,
    time: Res<'_, Time<Real>>,
) {
    if observed.0.load(Ordering::Acquire) {
        info!("Early shutdown probe observed real in-flight pipeline compilation");
        exit.send(if options.error_exit {
            AppExit::Error(std::num::NonZeroU8::MIN.saturating_add(6))
        } else {
            AppExit::Success
        });
    } else if time.elapsed_secs() > 60.0 {
        error!("Probe failed to observe real pipeline compilation within 60 seconds");
        exit.send(AppExit::error());
    }
}

fn main() -> AppExit {
    let options = Options::parse();
    let scenario = client::runtime_battle::DEFAULT_BATTLE_SCENARIO.as_ref();
    let mut app = match client::build_graphical_app(client::ClientMode::LiveBattle, None, scenario)
    {
        Ok(app) => app,
        Err(error) => {
            eprintln!("Cannot start actual renderer shutdown probe: {error}");
            return AppExit::error();
        }
    };
    let observed = CompileObserved::default();
    app.insert_resource(options)
        .insert_resource(observed.clone())
        .add_systems(Last, request_early_exit);
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        error!("Production render app missing in native shutdown probe");
        return AppExit::error();
    };
    render.insert_resource(observed).add_systems(
        Render,
        inspect_compilation
            .after(RenderSet::Render)
            .before(RenderSet::Cleanup),
    );
    client::render_compatibility::run_render_app(&mut app)
}
