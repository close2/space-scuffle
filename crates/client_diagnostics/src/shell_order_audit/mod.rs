//! Opt-in actual transparent-phase diagnosis with conservative screenshot intervals.
//!
//! Natural observation never modifies draw order. The normal client does not
//! install this module; bounded capture evidence is not gameplay profiling.

mod assets;
mod association;
pub mod fixture;
#[cfg(test)]
mod gpu_tests;
pub(crate) mod interval;
mod model;
mod phase;
mod source;
#[cfg(test)]
mod tests;

use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::prelude::*;
use bevy::render::render_phase::sort_phase_system;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSet};
pub use model::{AuditConfig, AuditError, FixtureOrder, FixtureSpec};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Resource, Clone)]
/// Cross-world diagnostic owner; absent from ordinary observer-off applications.
pub(crate) struct AuditBridge(Arc<Mutex<Shared>>);

struct Shared {
    collector: interval::Collector,
    config: AuditConfig,
    fault: Option<AuditError>,
    assets: assets::Inventory,
    source_pending: bool,
}

impl AuditBridge {
    fn lock(&self) -> Result<MutexGuard<'_, Shared>, AuditError> {
        self.0.lock().map_err(|_| AuditError::Poisoned)
    }
    /// Returns the first retained observer fault without altering uploader or renderer state.
    pub(crate) fn failure(&self) -> Result<(), AuditError> {
        self.lock()?.fault.clone().map_or(Ok(()), Err)
    }
    /// Reports whether a complete ready lower-bound epoch exists and no fault is retained.
    pub(crate) fn ready(&self) -> Result<bool, AuditError> {
        self.failure()?;
        let state = self.lock()?;
        Ok(!state.source_pending && state.collector.ready())
    }
    /// Begins a bounded candidate interval and returns its monotonic request nonce.
    pub(crate) fn arm(&self) -> Result<u64, AuditError> {
        self.failure()?;
        self.lock()?.collector.arm()
    }
    /// Binds the request nonce to the actual screenshot entity.
    pub(crate) fn bind(&self, nonce: u64, screenshot: Entity) -> Result<(), AuditError> {
        self.lock()?.collector.bind(nonce, screenshot.to_bits())
    }
    /// Records the actual screenshot callback and requests a subsequent render closure.
    pub(crate) fn callback(&self, screenshot: Entity) -> Result<(), AuditError> {
        self.lock()?.collector.callback(screenshot.to_bits())
    }
    /// Returns a fully closed interval with complete source accounting, or None while pending.
    pub(crate) fn finish(&self) -> Result<Option<interval::IntervalReport>, AuditError> {
        self.failure()?;
        let mut state = self.lock()?;
        state
            .collector
            .finish()?
            .map(|report| attach_assets(report, &state.assets))
            .transpose()
    }
    /// Releases a closed report only after successful bounded publication and record update.
    pub(crate) fn acknowledge_publication(&self) -> Result<(), AuditError> {
        self.failure()?;
        self.lock()?.collector.acknowledge_publication()
    }
    /// Copies the available partial evidence and identical source accounting for failure persistence.
    pub(crate) fn incomplete(&self) -> Result<Option<interval::IntervalReport>, AuditError> {
        let state = self.lock()?;
        state
            .collector
            .incomplete()
            .map(|report| attach_assets(report, &state.assets))
            .transpose()
    }
    /// Returns the declared exact compact publication cap in bytes.
    pub(crate) fn publication_limit(&self) -> Result<usize, AuditError> {
        Ok(self.lock()?.config.max_bytes)
    }
    /// Writes the retained exact CPU source byte sidecars beneath the output directory.
    pub(crate) fn persist_sources(&self, out: &std::path::Path) -> Result<(), AuditError> {
        self.lock()?.assets.persist(out)
    }
    fn fail(&self, error: AuditError) {
        if let Ok(mut state) = self.0.lock() {
            if state.fault.is_none() {
                state.fault = Some(error);
            }
        }
    }
}

fn attach_assets(
    mut report: interval::IntervalReport,
    assets: &assets::Inventory,
) -> Result<interval::IntervalReport, AuditError> {
    report.assets.clone_from(&assets.records);
    report.retained_bytes = report
        .retained_bytes
        .checked_add(assets.serialized_bytes)
        .ok_or(AuditError::Budget("report component accounting overflow"))?;
    report.raw_source_bytes = assets.raw_bytes;
    Ok(report)
}

#[derive(Resource, Default)]
struct Extracted {
    epoch: u64,
    source: Option<model::SourceFrame>,
    queued: Option<model::EpochEvidence>,
}

/// Installs only the explicitly requested bounded observer into the real renderer.
pub(crate) fn install(app: &mut App, config: AuditConfig) -> Result<(), AuditError> {
    if let Some(fixture) = &config.fixture {
        // The two-shell experiment measures shell order alone, so it turns smog off through the
        // same control a player uses.
        app.world_mut()
            .resource_mut::<client::LabState>()
            .opt_smog_volume = false;
        app.insert_resource(fixture::ControlledFixture(fixture.clone()))
            .add_systems(
                PostUpdate,
                fixture::place
                    .before(TransformSystem::TransformPropagate)
                    .before(bevy::render::view::VisibilitySystems::VisibilityPropagate),
            );
    }
    let bridge = AuditBridge(Arc::new(Mutex::new(Shared {
        collector: interval::Collector::new(config.clone())?,
        config,
        fault: None,
        assets: assets::Inventory::default(),
        source_pending: true,
    })));
    app.insert_resource(bridge.clone());
    let render = app
        .get_sub_app_mut(RenderApp)
        .ok_or_else(|| AuditError::Source("actual render application absent".into()))?;
    render
        .insert_resource(bridge)
        .init_resource::<Extracted>()
        .add_systems(ExtractSchedule, extract)
        .add_systems(
            Render,
            phase::queued
                .in_set(RenderSet::PhaseSort)
                .before(sort_phase_system::<Transparent3d>),
        )
        .add_systems(
            Render,
            phase::sorted
                .in_set(RenderSet::PhaseSort)
                .after(sort_phase_system::<Transparent3d>),
        );
    Ok(())
}

fn extract(world: &mut World) {
    let bridge = world.resource::<AuditBridge>().clone();
    if bridge.failure().is_err() {
        return;
    }
    let result = world.resource_scope(
        |world, mut main: Mut<'_, MainWorld>| -> Result<(), AuditError> {
            let mut state = bridge.lock()?;
            let limit = state.config.max_source_bytes;
            state.assets.begin_epoch();
            let source = match source::capture(&mut main, &mut state.assets, limit) {
                Ok(source) => source,
                Err(AuditError::NotReady(_)) if !state.collector.active() => None,
                Err(error) => return Err(error),
            };
            if source.is_none() && state.collector.active() {
                return Err(AuditError::Lifecycle(
                    "source vanished during active interval",
                ));
            }
            state.source_pending = source.is_none();
            let asset_bytes = state.assets.serialized_bytes;
            state.collector.reserve_asset_bytes(asset_bytes)?;
            drop(state);
            let epoch = if source.is_some() {
                bridge.lock()?.collector.extract_epoch()?
            } else {
                0
            };
            let mut extracted = world.resource_mut::<Extracted>();
            if extracted.queued.is_some() {
                return Err(AuditError::Lifecycle("previous epoch lacks sorted phase"));
            }
            extracted.epoch = epoch;
            extracted.source = source;
            Ok(())
        },
    );
    if let Err(error) = result {
        bridge.fail(error);
    }
}
