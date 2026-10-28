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
mod pipelines;
mod transport;
mod ui_attachment;

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
pub fn run_capture(options: CaptureOptions) -> Result<AppExit, CaptureError> {
    let mut app =
        crate::build_graphical_app(crate::ClientMode::LiveBattle, None, &options.scenario)?;
    let root = app
        .world()
        .resource::<crate::ResolvedAssetsRoot>()
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
        .resource::<crate::runtime_battle::BattleDefinition>()
        .source
        .clone();
    let audio_policy = *app.world().resource::<crate::presentation::AudioPolicy>();
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
        .resource_mut::<crate::LabState>()
        .battle_paused = true;
    app.world_mut()
        .resource_mut::<crate::LabState>()
        .simulation_speed = 1;
    let evidence = pipelines::PipelineEvidence::new(app.world().resource::<AssetServer>());
    app.insert_resource(session)
        .insert_resource(crate::media::MediaFailurePolicy::new(true))
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
            driver::control.in_set(crate::battle_pipeline::BattleSet::ExternalControl),
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
    let exit = crate::render_compatibility::run_render_app(&mut app);
    if matches!(exit, AppExit::Success) && !completed.load(std::sync::atomic::Ordering::Acquire) {
        return Err(CaptureError::Verification(
            "application exited before all capture callbacks completed".into(),
        ));
    }
    Ok(exit)
}

fn isolate_ui_input(
    mut inputs: Query<'_, '_, (&mut bevy_egui::EguiInput, &mut bevy_egui::EguiContext)>,
    mut session: ResMut<'_, driver::Session>,
    time: Res<'_, Time>,
) {
    for (mut input, mut context) in &mut inputs {
        input.0.events.clear();
        input.0.modifiers = bevy_egui::egui::Modifiers::default();
        // UI fade-in must settle while authoritative/visual world time remains
        // zero. Its separate startup clock advances by deterministic 40 ms frames,
        // then freezes at twice the real configured egui animation duration.
        let settling = f64::from(context.get_mut().style().animation_time) * 2.0;
        let startup = session.ui_startup_seconds();
        session.ui_settled = startup >= settling;
        input.0.time = Some(time.elapsed_secs_f64() + startup.min(settling));
    }
}
