//! Exact production shader-handle readiness evidence from the actual render world.
//!
//! Atomic counters avoid per-frame allocations and do not enter simulation state.
//! An unrelated UI pipeline never satisfies a terrain, bubble or combat gate.

use bevy::prelude::*;
use bevy::render::render_resource::{
    CachedPipelineState, PipelineCache, PipelineCacheError, PipelineDescriptor,
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

pub(super) const SHADERS: [&str; 3] = [
    "shaders/terrain.wgsl",
    "shaders/bubble_dome.wgsl",
    "shaders/combat_fx.wgsl",
];

#[derive(Resource, Clone)]
pub(super) struct PipelineEvidence {
    handles: [Handle<Shader>; 3],
    counts: Arc<[AtomicUsize; 3]>,
    failed: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
}

impl PipelineEvidence {
    pub(super) fn new(server: &AssetServer) -> Self {
        Self {
            handles: SHADERS.map(|path| server.load(path)),
            counts: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            failed: Arc::default(),
            pending: Arc::default(),
        }
    }
    pub(super) fn counts(&self) -> [usize; 3] {
        std::array::from_fn(|index| self.counts[index].load(Ordering::Acquire))
    }
    pub(super) fn report_failure(&self, message: &str) {
        if !self.failed.swap(true, Ordering::AcqRel) {
            error!("Capture render integration failed: {message}");
        }
    }
    pub(super) fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
    pub(super) fn ready(&self, mask: u8) -> bool {
        !self.failed()
            && !self.pending.load(Ordering::Acquire)
            && self
                .counts()
                .iter()
                .enumerate()
                .all(|(index, count)| mask & (1 << index) == 0 || *count > 0)
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub(super) fn inspect(cache: Res<'_, PipelineCache>, evidence: Res<'_, PipelineEvidence>) {
    let mut counts = [0; 3];
    let mut pending = cache.waiting_pipelines().next().is_some();
    for pipeline in cache.pipelines() {
        match &pipeline.state {
            CachedPipelineState::Ok(_) => {
                if let PipelineDescriptor::RenderPipelineDescriptor(descriptor) =
                    &pipeline.descriptor
                {
                    for (index, shader) in evidence.handles.iter().enumerate() {
                        if descriptor
                            .fragment
                            .as_ref()
                            .is_some_and(|fragment| fragment.shader.id() == shader.id())
                        {
                            counts[index] += 1;
                        }
                    }
                }
            }
            CachedPipelineState::Queued
            | CachedPipelineState::Creating(_)
            | CachedPipelineState::Err(
                PipelineCacheError::ShaderNotLoaded(_)
                | PipelineCacheError::ShaderImportNotYetAvailable,
            ) => pending = true,
            CachedPipelineState::Err(error) => {
                if !evidence.failed.swap(true, Ordering::AcqRel) {
                    error!("Capture production pipeline failed: {error:?}");
                }
            }
        }
    }
    for (count, atomic) in counts.into_iter().zip(evidence.counts.iter()) {
        atomic.store(count, Ordering::Release);
    }
    evidence.pending.store(pending, Ordering::Release);
}
