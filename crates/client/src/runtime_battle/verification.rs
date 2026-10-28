//! Real runtime pipeline evidence shared across Bevy's main and render worlds.
//!
//! A failed GPU pipeline terminates the game verification run with failure, rather
//! than allowing a successful frame-limit exit to conceal a missing shader effect.

use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache, PipelineCacheError};
use bevy::render::{Render, RenderApp};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// GPU pipeline readiness and failure evidence from the real render sub-application.
#[derive(Resource, Clone, Default)]
pub struct BattleRenderStatus {
    ready: Arc<AtomicUsize>,
    failed: Arc<AtomicBool>,
    settled: Arc<AtomicBool>,
}

impl BattleRenderStatus {
    /// Number of pipelines that the actual rendering device compiled successfully.
    #[must_use]
    pub fn ready_pipelines(&self) -> usize {
        self.ready.load(Ordering::Acquire)
    }

    /// True only once at least one GPU pipeline compiled and no pipeline is pending.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.settled.load(Ordering::Acquire) && self.ready_pipelines() > 0
    }

    /// Whether any actual GPU shader pipeline reported a compilation error.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

/// Registers real GPU pipeline inspection when the graphical renderer is available.
pub struct BattleRenderVerificationPlugin;

impl Plugin for BattleRenderVerificationPlugin {
    fn build(&self, app: &mut App) {
        let status = BattleRenderStatus::default();
        app.insert_resource(status.clone());
        app.add_systems(Update, fail_on_pipeline_error);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(status)
                .add_systems(Render, inspect_pipelines);
        }
    }
}

fn inspect_pipelines(cache: Res<'_, PipelineCache>, status: Res<'_, BattleRenderStatus>) {
    let mut ready = 0;
    let mut pending = cache.waiting_pipelines().next().is_some();
    for pipeline in cache.pipelines() {
        match &pipeline.state {
            CachedPipelineState::Ok(_) => ready += 1,
            CachedPipelineState::Err(
                PipelineCacheError::ShaderNotLoaded(_)
                | PipelineCacheError::ShaderImportNotYetAvailable,
            )
            | CachedPipelineState::Queued
            | CachedPipelineState::Creating(_) => pending = true,
            CachedPipelineState::Err(error) if !status.failed.swap(true, Ordering::AcqRel) => {
                error!("Runtime GPU pipeline failed: {error:?}");
            }
            CachedPipelineState::Err(_) => {}
        }
    }
    status.ready.store(ready, Ordering::Release);
    status.settled.store(!pending, Ordering::Release);
}

fn fail_on_pipeline_error(
    status: Res<'_, BattleRenderStatus>,
    time: Res<'_, Time>,
    mut pending_seconds: Local<'_, f32>,
    mut exit: EventWriter<'_, AppExit>,
) {
    // Asset imports are asynchronous, but a missing shader must not stall forever.
    // The frame-limit verifier can impose a stricter capture-specific deadline.
    const READINESS_TIMEOUT_SECONDS: f32 = 30.0;
    if status.is_ready() {
        *pending_seconds = 0.0;
    } else {
        *pending_seconds += time.delta_secs();
    }
    if status.failed() {
        exit.send(AppExit::error());
    } else if *pending_seconds >= READINESS_TIMEOUT_SECONDS {
        error!("Runtime GPU pipelines did not settle within {READINESS_TIMEOUT_SECONDS} seconds");
        exit.send(AppExit::error());
    }
}
