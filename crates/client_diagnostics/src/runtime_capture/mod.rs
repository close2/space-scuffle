//! Reproducible real-GPU game captures through the shared production application.
//!
//! This tool drives ordinary camera/tab/reset state controls, never OS input or a
//! replacement world. Every checkpoint requires real assets, exact shader pipelines,
//! visible scene subjects and acknowledged readback before an artifact can pass.

mod artifacts;
mod config;
mod driver;
mod media;
mod ownership;
pub(crate) mod pipelines;
mod transport;
pub(crate) mod ui_attachment;

pub use config::{
    CaptureError, CaptureOptions, CaptureScript, Checkpoint, Goal, TransportCaptureSpec,
};

use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSet};
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

/// Runs the actual production client with explicit deterministic capture controls.
///
/// # Errors
/// Returns startup/script/output errors. Runtime GPU/state/readback failures emit
/// `AppExit::Error` and retain an incomplete manifest with diagnostic evidence.
#[expect(
    clippy::cast_precision_loss,
    reason = "capture coordinates are bounded by the window size and frame budget"
)]
pub fn run_capture(options: CaptureOptions) -> Result<AppExit, CaptureError> {
    let mut app =
        client::build_graphical_app(client::ClientMode::LiveBattle, None, &options.scenario)?;
    let root = app
        .world()
        .resource::<client::ResolvedAssetsRoot>()
        .as_path()
        .to_owned();
    let script_path = root.join(&options.script);
    let script = CaptureScript::load(&script_path)?;
    if let Some(config) = &script.shell_audit {
        crate::shell_order_audit::install(&mut app, config.clone())?;
    }
    std::fs::create_dir(&options.out).map_err(|source| CaptureError::Io {
        path: options.out.clone(),
        source,
    })?;
    let scenario = app
        .world()
        .resource::<client::runtime_battle::BattleDefinition>()
        .source
        .clone();
    let audio_policy = *app.world().resource::<client::presentation::AudioPolicy>();
    let session = driver::Session::new(
        script.clone(),
        scenario,
        script_path,
        root,
        options.out,
        audio_policy,
    );
    session.manifest()?;
    let completed = session.completed.clone();
    let mut query = app.world_mut().query::<&mut Window>();
    let mut window = query
        .get_single_mut(app.world_mut())
        .map_err(|error| CaptureError::Verification(error.to_string()))?;
    window.resolution.set_scale_factor_override(Some(1.0));
    window
        .resolution
        .set(script.resolution[0] as f32, script.resolution[1] as f32);
    window.title = "Space Scuffle — production capture".into();
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = true;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .simulation_speed = 1;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .smog_bouncing_active = false;
    app.world_mut()
        .resource_mut::<client::LabState>()
        .opt_smog_volume = false;
    if let Some(mut smog_config) = app
        .world_mut()
        .get_resource_mut::<client::smog::SmogConfig>()
    {
        smog_config.bouncing_active = false;
        smog_config.enabled = false;
    }
    let evidence = pipelines::PipelineEvidence::new(app.world().resource::<AssetServer>());
    app.insert_resource(session)
        .insert_resource(client::media::MediaFailurePolicy::new(true))
        .insert_resource(evidence.clone())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .add_systems(
            PreUpdate,
            isolate_ui_input
                .after(bevy_egui::EguiPreUpdateSet::ProcessInput)
                .before(bevy_egui::EguiPreUpdateSet::BeginPass),
        )
        .add_systems(
            Update,
            driver::control.in_set(client::battle_pipeline::BattleSet::ExternalControl),
        )
        .add_systems(Last, driver::observe);
    let render = app
        .get_sub_app_mut(RenderApp)
        .ok_or_else(|| CaptureError::Verification("real render application missing".into()))?;
    render
        .insert_resource(evidence)
        .add_systems(Render, pipelines::inspect)
        .add_systems(
            Render,
            ui_attachment::align_ui_attachment
                .after(bevy::render::view::prepare_view_targets)
                .before(RenderSet::Render),
        );
    let exit = client::render_compatibility::run_render_app(&mut app);
    if matches!(exit, AppExit::Success) && !completed.load(std::sync::atomic::Ordering::Acquire) {
        return Err(CaptureError::Verification(
            "application exited before all capture callbacks completed".into(),
        ));
    }
    Ok(exit)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn isolate_ui_input(
    mut inputs: Query<'_, '_, (&mut bevy_egui::EguiInput, &mut bevy_egui::EguiContext)>,
    mut session: ResMut<'_, driver::Session>,
    time: Res<'_, Time>,
) {
    let startup = session.ui_startup_seconds();
    if let Some(settled) = isolate_egui_inputs(&mut inputs, time.elapsed_secs_f64(), startup) {
        session.ui_settled = settled;
    }
}

/// Clears every egui input event and modifier and gives egui a deterministic clock.
///
/// UI fade-in must settle while authoritative and visual world time remain zero. The separate
/// `startup` clock (seconds) advances by deterministic 40 ms frames, then freezes at twice the
/// configured egui animation duration; egui's time is `elapsed` plus that clock. Returns whether
/// the fade-in has settled, or `None` when there is no egui context. Shared by the capture and the
/// showcase tools.
pub(crate) fn isolate_egui_inputs(
    inputs: &mut Query<'_, '_, (&mut bevy_egui::EguiInput, &mut bevy_egui::EguiContext)>,
    elapsed: f64,
    startup: f64,
) -> Option<bool> {
    let mut settled = None;
    for (mut input, mut context) in inputs.iter_mut() {
        input.0.events.clear();
        input.0.modifiers = bevy_egui::egui::Modifiers::default();
        let settling = f64::from(context.get_mut().style().animation_time) * 2.0;
        settled = Some(startup >= settling);
        input.0.time = Some(elapsed + startup.min(settling));
    }
    settled
}
