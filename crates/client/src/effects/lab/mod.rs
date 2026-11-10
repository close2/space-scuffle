//! The effect lab: one effect on real terrain under the battle camera, stepped in
//! `Time<Virtual>` and captured at exact ages (plan `2026-11-09-effect-visualization` §6.5,
//! §10.2).
//!
//! # Responsibility
//! Owns [`EffectLabPlugin`] and [`lab_app`]: the production presentation plugins, the
//! production [`super::EffectsPlugin`] with the shipped vocabulary (or an explicit one), a stage
//! of terrain swatches lit by the theme ([`scene`]), and the driver that runs a
//! [`LabProgram`] ([`driver`]) and writes one PNG per shot.
//!
//! # How it fits
//! `client_diagnostics`' `battle_capture --effect-lab` builds a contact-sheet program and
//! assembles the PNGs into a sheet; the effect GPU tests build their own programs in a child
//! process of the GPU harness and measure the PNGs. Both run the same code, so what the tests
//! check is what the sheets show.
//!
//! # Invariants
//! - Effect time is `Time<Virtual>`, advanced only by the driver ([`program::next_step`]); a
//!   program captures the same pictures on every run of one driver.
//! - Nothing here names an effect, a unit or a terrain layer (charter C5).

pub mod driver;
pub mod program;
pub mod scene;

pub use driver::{LabDriver, LabPhase, LabRenderIdle};
pub use program::{
    LabGround, LabProgram, LabProgramError, LabShot, LabSubject, LabTake, LabZoom, CONTACT_AGES,
    SEQUENCE_STEP,
};
pub use scene::{LabScene, LabSceneError};

use super::systems::EffectCatalogSource;
use super::EffectsPlugin;
use crate::render_compatibility::RenderCompatibilityPlugin;
use crate::ResolvedAssetsRoot;
use bevy::prelude::*;
use bevy::render::{Render, RenderApp};
use bevy::time::TimeUpdateStrategy;
use std::path::PathBuf;
use std::time::Duration;

/// Runs a [`LabProgram`] in an app that has the presentation plugins and the effect plugin.
pub struct EffectLabPlugin {
    /// The program.
    pub program: LabProgram,
}

impl Plugin for EffectLabPlugin {
    fn build(&self, app: &mut App) {
        let idle = LabRenderIdle::default();
        app.insert_resource(self.program.clone())
            .insert_resource(LabDriver::default())
            .insert_resource(idle.clone())
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .add_systems(Startup, setup_stage)
            .add_systems(Update, (driver::drive_lab, draw_overlay).chain());
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(idle)
                .add_systems(Render, driver::track_render_idle);
        }
    }
}

/// Spawns the stage; a failure ends the app with an error.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn setup_stage(
    mut commands: Commands<'_, '_>,
    server: Res<'_, AssetServer>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    root: Res<'_, ResolvedAssetsRoot>,
    mut driver: ResMut<'_, LabDriver>,
) {
    match scene::spawn_scene(&mut commands, &server, &mut meshes, &mut materials, &root) {
        Ok(stage) => {
            commands.insert_resource(stage);
        }
        Err(error) => driver.failed = Some(error.to_string()),
    }
}

/// Draws the reference overlay in the frames the driver asks for it.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn draw_overlay(
    driver: Res<'_, LabDriver>,
    program: Res<'_, LabProgram>,
    mut gizmos: Gizmos<'_, '_>,
) {
    if !driver.overlay {
        return;
    }
    let bound = &program.subject.bound;
    let radii = [
        bound.get(super::catalog::schema::Binding::SplashFull),
        bound.get(super::catalog::schema::Binding::SplashReach),
    ];
    scene::draw_overlay(&mut gizmos, &program.subject, radii);
}

/// A lab app that cannot be built.
#[derive(Debug, thiserror::Error)]
pub enum LabAppError {
    /// The program breaks an invariant.
    #[error(transparent)]
    Program(#[from] LabProgramError),
    /// The assets root has no Bevy file path.
    #[error(transparent)]
    Assets(#[from] crate::assets::AssetsDirError),
    /// The output directory could not be created.
    #[error("effect lab output {path}: {source}")]
    Output {
        /// The directory.
        path: PathBuf,
        /// The failure.
        #[source]
        source: std::io::Error,
    },
}

/// The lab app: an invisible `size` window, the production presentation and effect plugins
/// reading `vocabulary` (the shipped one when `None`) under `root`, and `program`'s driver.
/// Run it with [`crate::render_compatibility::run_render_app`].
///
/// # Errors
/// [`LabAppError`] when the program is invalid, the assets root unusable or the output
/// directory cannot be created.
pub fn lab_app(
    root: ResolvedAssetsRoot,
    program: LabProgram,
    vocabulary: Option<PathBuf>,
    size: UVec2,
) -> Result<App, LabAppError> {
    program.validate()?;
    std::fs::create_dir_all(&program.out).map_err(|source| LabAppError::Output {
        path: program.out.clone(),
        source,
    })?;
    let mut window_backend = bevy::winit::WinitPlugin::<bevy::winit::WakeUp>::default();
    window_backend.run_on_any_thread = true;
    let mut window = Window {
        visible: false,
        title: "Space Scuffle — effect lab".into(),
        ..default()
    };
    window.resolution.set_scale_factor_override(Some(1.0));
    #[expect(
        clippy::cast_precision_loss,
        reason = "window sizes are far below 2^24"
    )]
    window.resolution.set(size.x as f32, size.y as f32);
    let mut app = App::new();
    app.add_plugins(
        crate::presentation::GamePresentationPlugins
            .set(AssetPlugin {
                file_path: root.bevy_file_path()?.to_owned(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(window),
                ..default()
            })
            .set(window_backend),
    )
    .add_plugins((RenderCompatibilityPlugin, EffectsPlugin))
    .add_event::<crate::CombatVisualEvent>()
    .insert_resource(root);
    if let Some(vocabulary) = vocabulary {
        app.insert_resource(EffectCatalogSource(vocabulary));
    }
    app.add_plugins(EffectLabPlugin { program });
    Ok(app)
}
