//! Frozen-history, real-render fixtures for actual view-bound mist illumination.
//!
//! Every pre-existing light is inventoried and hidden before controlled rigs are
//! admitted. Three screenshot callbacks gate condition transitions. The history,
//! media clock, packet geometry and buffer ownership remain fixed while actual
//! visibility, parent transforms, view layers or mapping readiness change.

use super::callback_gate::{CallbackGate, GateError, PacketProof};
use super::capture::{CaptureCase, CaptureOptions, CaptureProgress};
use super::light_probe::{ProbeError, ProbeEvidence, ProbeReport, ProbeRequest};
use super::{MistPool, MistRibbon, MistStyle};
use crate::BubbleInstance;
use bevy::prelude::*;
use bevy::render::camera::Viewport;
use bevy::render::primitives::Aabb;
use bevy::render::view::RenderLayers;
use bevy::render::RenderApp;
use bevy::time::TimeUpdateStrategy;
use clap::ValueEnum;
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, Instant};

/// Bounded actual-render condition sequence; all use the nonbenchmark Close scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
pub enum ViewLightCase {
    /// Hidden bright rig becomes visible, then hidden again.
    Hidden,
    /// A nonparallel directional-light parent rotates 90 degrees and returns.
    Parented,
    /// Two actual views exchange exclusive light layers, then restore them.
    Layered,
    /// All eligible lights disappear, then the same rig returns.
    Zero,
    /// New view lighting reaches a retained packet during genuine held GPU callbacks.
    Stalled,
}

/// A fixture failure invalidates screenshots and keeps the original nonzero exit.
#[derive(Debug, thiserror::Error)]
pub(super) enum LightCaptureError {
    /// Unsupported combination could not isolate the requested lighting contract.
    #[error("view-light verification requires nonbenchmark Close without owner reset")]
    Options,
    /// A real preparation/readback or authored invariant failed.
    #[error("view-light capture invariant failed: {0}")]
    Invariant(&'static str),
    /// The opt-in fixture exhausted its real wall-time readiness budget.
    #[error("view-light verification exceeded 180 wall seconds")]
    Deadline,
    /// Actual dynamic uniform readback failed.
    #[error(transparent)]
    Probe(#[from] ProbeError),
    /// Actual mapping lifecycle evidence failed.
    #[error(transparent)]
    Gate(#[from] GateError),
    /// Persisting exact evidence failed.
    #[error("view-light evidence write failed: {0}")]
    Write(#[from] std::io::Error),
    /// Encoding bounded evidence failed.
    #[error("view-light evidence serialization failed: {0}")]
    Serialize(#[from] ron::Error),
}

#[derive(Component)]
struct FixtureParent;

#[derive(Serialize)]
struct Checkpoint {
    image: usize,
    presentation_frame: u32,
    pool_seconds: f32,
    packet_sequence: u64,
    packet_file: String,
    packet_bytes: usize,
    held_callbacks: usize,
    target_ids: String,
    staging_ids: String,
    actual_view_lights: ProbeReport,
}

#[derive(Serialize)]
struct Report<'a> {
    mode: ViewLightCase,
    hidden_original_rigs: Vec<u64>,
    checkpoints: &'a [Checkpoint],
}
enum Terminal {
    Failed,
    InvalidInjected,
}

/// Capture-only progression and bounded immutable evidence.
#[derive(Resource)]
pub(super) struct LightCapture {
    mode: ViewLightCase,
    started: Instant,
    initialized: bool,
    old_lights: Vec<Entity>,
    rigs: [Option<Entity>; 2],
    parent: Option<Entity>,
    cameras: [Option<Entity>; 2],
    applied: Option<usize>,
    ready: Option<usize>,
    frozen_clock: Option<f32>,
    freeze_pending: bool,
    baseline: Option<PacketProof>,
    retained: Option<PacketProof>,
    checkpoints: Vec<Checkpoint>,
    terminal: Option<Terminal>,
}

impl LightCapture {
    /// Whether actual view-buffer preparation and numeric checks admit this screenshot.
    pub(super) fn ready(&self, image: usize) -> bool {
        self.terminal.is_none() && self.ready == Some(image)
    }
    /// Whether a sticky fixture failure forbids further screenshots or success exits.
    pub(super) const fn failed(&self) -> bool {
        matches!(self.terminal, Some(Terminal::Failed))
    }
    /// Writes all three complete conditions only after their screenshot callbacks finish.
    pub(super) fn finish(&self, out: &Path) -> Result<(), LightCaptureError> {
        if self.terminal.is_some() || self.checkpoints.len() != 3 {
            return Err(LightCaptureError::Invariant(
                "three verified light checkpoints required",
            ));
        }
        let report = Report {
            mode: self.mode,
            hidden_original_rigs: self
                .old_lights
                .iter()
                .map(|entity| entity.to_bits())
                .collect(),
            checkpoints: &self.checkpoints,
        };
        std::fs::write(
            out.join("view-lights.ron"),
            ron::ser::to_string_pretty(&report, ron::ser::PrettyConfig::default())?,
        )?;
        Ok(())
    }
}

/// Rejects combinations that would mix benchmark, ownership and lighting contracts.
pub(super) fn validate(options: &CaptureOptions) -> Result<(), LightCaptureError> {
    if options.verify_invalid_view_light
        && options.verify_view_lights != Some(ViewLightCase::Stalled)
    {
        return Err(LightCaptureError::Options);
    }
    if options.verify_view_lights.is_some()
        && (options.case != CaptureCase::Close
            || options.benchmark_emitters.is_some()
            || options.verify_owner_reset)
    {
        return Err(LightCaptureError::Options);
    }
    Ok(())
}

/// Installs bounded fixture resources and the real prepared-light readback observer.
pub(super) fn install(
    app: &mut App,
    root: &Path,
    mode: ViewLightCase,
) -> Result<(), Box<dyn std::error::Error>> {
    let style = MistStyle::load(&root.join("catalog/mist.ron"))?;
    let layout = super::gpu::PacketLayout::new(style.max_emitters, style.samples_per_emitter)?;
    let gate = CallbackGate::new(layout)?;
    super::light_probe::install(app, root, style.reference_illuminance_lux)?;
    app.insert_resource(gate.clone())
        .insert_resource(LightCapture {
            mode,
            started: Instant::now(),
            initialized: false,
            old_lights: Vec::with_capacity(64),
            rigs: [None; 2],
            parent: None,
            cameras: [None; 2],
            applied: None,
            ready: None,
            frozen_clock: None,
            freeze_pending: false,
            baseline: None,
            retained: None,
            checkpoints: Vec::with_capacity(3),
            terminal: None,
        })
        .add_systems(Update, drive.after(super::capture::CaptureSystems::Motion));
    app.sub_app_mut(RenderApp).insert_resource(gate);
    Ok(())
}

type FixtureLights<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut DirectionalLight,
        &'static mut Visibility,
        &'static mut Transform,
        Option<&'static mut RenderLayers>,
    ),
    (Without<Camera>, Without<MistRibbon>),
>;
type FixtureCameras<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Camera,
        &'static Camera3d,
        &'static Projection,
        &'static mut Transform,
        &'static Msaa,
    ),
    Without<DirectionalLight>,
>;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive(
    mut commands: Commands<'_, '_>,
    options: Res<'_, CaptureOptions>,
    progress: Res<'_, CaptureProgress>,
    mut state: ResMut<'_, LightCapture>,
    pool: Res<'_, MistPool>,
    mut clock: ResMut<'_, TimeUpdateStrategy>,
    gate: Res<'_, CallbackGate>,
    evidence: Res<'_, ProbeEvidence>,
    mut request: ResMut<'_, ProbeRequest>,
    mut lights: FixtureLights<'_, '_>,
    mut cameras: FixtureCameras<'_, '_>,
    mut parents: Query<
        '_,
        '_,
        &mut Transform,
        (
            With<FixtureParent>,
            Without<DirectionalLight>,
            Without<Camera>,
        ),
    >,
    actors: Query<'_, '_, (&BubbleInstance, &GlobalTransform)>,
    ribbons: Query<
        '_,
        '_,
        (&Aabb, &GlobalTransform, &Visibility),
        (With<MistRibbon>, Without<DirectionalLight>),
    >,
    mut exit: EventWriter<'_, AppExit>,
) {
    let result = (|| -> Result<(), LightCaptureError> {
        if state.terminal.is_some() || progress.complete {
            return Ok(());
        }
        if state.started.elapsed() > Duration::from_secs(180) {
            return Err(LightCaptureError::Deadline);
        }
        if !state.initialized {
            for (entity, _, mut visibility, _, _) in &mut lights {
                if state.old_lights.len() == state.old_lights.capacity() {
                    return Err(LightCaptureError::Invariant(
                        "original light inventory capacity",
                    ));
                }
                state.old_lights.push(entity);
                *visibility = Visibility::Hidden;
            }
            state.old_lights.sort_unstable();
            let (entity, mut camera, camera3d, projection, transform, msaa) =
                cameras.get_single_mut().map_err(|_| {
                    LightCaptureError::Invariant("one initial actual Close camera required")
                })?;
            state.cameras[0] = Some(entity);
            commands
                .entity(entity)
                .insert(RenderLayers::from_layers(&[0, 1]));
            if state.mode == ViewLightCase::Layered {
                camera.viewport = Some(Viewport {
                    physical_position: UVec2::ZERO,
                    physical_size: UVec2::new(640, 720),
                    ..default()
                });
                let mut second = camera.clone();
                second.order += 1;
                second.viewport = Some(Viewport {
                    physical_position: UVec2::new(640, 0),
                    physical_size: UVec2::new(640, 720),
                    ..default()
                });
                state.cameras[1] = Some(
                    commands
                        .spawn((
                            second,
                            camera3d.clone(),
                            projection.clone(),
                            *transform,
                            *msaa,
                            RenderLayers::from_layers(&[0, 2]),
                        ))
                        .id(),
                );
            }
            let parent = commands
                .spawn((Transform::default(), Visibility::Inherited, FixtureParent))
                .id();
            state.parent = Some(parent);
            let direction = if state.mode == ViewLightCase::Parented {
                Vec3::new(1.0, 2.0, 3.0).normalize()
            } else {
                Vec3::Y
            };
            let white = commands
                .spawn((
                    DirectionalLight {
                        illuminance: 10_000.0,
                        ..default()
                    },
                    Transform::from_rotation(Quat::from_rotation_arc(Vec3::Z, direction)),
                    RenderLayers::layer(1),
                ))
                .id();
            commands.entity(parent).add_child(white);
            let red = commands
                .spawn((
                    DirectionalLight {
                        color: Color::linear_rgb(1.0, 0.0, 0.0),
                        illuminance: 20_000.0,
                        ..default()
                    },
                    Transform::from_rotation(Quat::from_rotation_arc(Vec3::Z, Vec3::Y)),
                    if state.mode == ViewLightCase::Layered {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                    RenderLayers::layer(if state.mode == ViewLightCase::Layered {
                        2
                    } else {
                        1
                    }),
                ))
                .id();
            state.rigs = [Some(white), Some(red)];
            state.initialized = true;
            return Ok(());
        }
        for (entity, _, visibility, _, _) in &mut lights {
            if state.old_lights.binary_search(&entity).is_ok() {
                if *visibility != Visibility::Hidden {
                    return Err(LightCaptureError::Invariant("original rig became visible"));
                }
            } else if !state.rigs.contains(&Some(entity)) {
                return Err(LightCaptureError::Invariant("undeclared directional rig"));
            }
        }
        if !progress.running || progress.elapsed_frames < options.fps * 2 {
            return Ok(());
        }
        *clock = TimeUpdateStrategy::ManualDuration(Duration::ZERO);
        if state.frozen_clock.is_none() {
            // The PostUpdate mist step for the last positive-delta frame still follows
            // this Update. Record its final clock in the following frozen frame.
            if !state.freeze_pending {
                state.freeze_pending = true;
                return Ok(());
            }
            state.frozen_clock = Some(pool.clock);
            if state.mode == ViewLightCase::Layered {
                let mut minimum = Vec3::splat(f32::INFINITY);
                let mut maximum = Vec3::splat(f32::NEG_INFINITY);
                for (bubble, transform) in &actors {
                    minimum = minimum.min(transform.translation() - Vec3::splat(bubble.0.radius));
                    maximum = maximum.max(transform.translation() + Vec3::splat(bubble.0.radius));
                }
                for (bounds, transform, visibility) in &ribbons {
                    if *visibility != Visibility::Visible {
                        continue;
                    }
                    for x in [-1.0, 1.0] {
                        for y in [-1.0, 1.0] {
                            for z in [-1.0, 1.0] {
                                let corner = transform.affine().transform_point3a(
                                    bounds.center
                                        + bounds.half_extents * bevy::math::Vec3A::new(x, y, z),
                                );
                                minimum = minimum.min(corner.into());
                                maximum = maximum.max(corner.into());
                            }
                        }
                    }
                }
                for (_, _, _, projection, mut transform, _) in &mut cameras {
                    let Projection::Perspective(projection) = projection else {
                        return Err(LightCaptureError::Invariant(
                            "perspective Close cameras required",
                        ));
                    };
                    *transform = super::benchmark::fit_camera(
                        minimum,
                        maximum,
                        640.0 / 720.0,
                        projection.fov,
                    )
                    .map_err(|_| LightCaptureError::Invariant("two-view camera fit failed"))?;
                }
            }
        }
        if state.frozen_clock != Some(pool.clock) {
            return Err(LightCaptureError::Invariant(
                "history clock advanced during frozen condition",
            ));
        }
        let image = progress.image;
        if progress.pending || state.ready == Some(image) {
            return Ok(());
        }
        if image >= 3 {
            return Err(LightCaptureError::Invariant(
                "image index outside bounded schedule",
            ));
        }
        if state.applied != Some(image) {
            if state.mode == ViewLightCase::Stalled && image == 1 {
                gate.hold();
                if gate.held()? != pool.style.gpu_staging_buffers {
                    return Ok(());
                }
                state.retained = Some(gate.snapshot()?);
                if options.verify_invalid_view_light {
                    let entity = state.rigs[0]
                        .ok_or(LightCaptureError::Invariant("invalid-light target missing"))?;
                    let accounting = pool
                        .publication
                        .lock()
                        .map_err(|_| {
                            LightCaptureError::Invariant(
                                "publication poisoned while arming invalid light",
                            )
                        })?
                        .accounting;
                    gate.arm_failure(entity, options.out.clone(), accounting)?;
                    let (_, mut light, _, _, _) = lights.get_mut(entity).map_err(|_| {
                        LightCaptureError::Invariant("actual invalid-light target missing")
                    })?;
                    light.illuminance = f32::NAN;
                    state.terminal = Some(Terminal::InvalidInjected);
                    return Ok(());
                }
            }
            if state.mode == ViewLightCase::Stalled && image == 2 {
                gate.release();
                let retained = state.retained.as_ref().ok_or(LightCaptureError::Invariant(
                    "retained packet evidence absent",
                ))?;
                if gate.sequence()? <= retained.sequence {
                    return Ok(());
                }
            }
            for (entity, mut light, mut visibility, _, layers) in &mut lights {
                if Some(entity) == state.rigs[0] {
                    let mut layers = layers
                        .ok_or(LightCaptureError::Invariant("fixture light layer missing"))?;
                    *visibility = if state.mode == ViewLightCase::Zero && image == 1 {
                        Visibility::Hidden
                    } else {
                        Visibility::Visible
                    };
                    *layers = RenderLayers::layer(
                        if state.mode == ViewLightCase::Layered && image == 1 {
                            2
                        } else {
                            1
                        },
                    );
                    if state.mode == ViewLightCase::Stalled && image > 0 {
                        light.color = Color::linear_rgb(1.0, 0.0, 0.0);
                    }
                } else if Some(entity) == state.rigs[1] {
                    let mut layers = layers
                        .ok_or(LightCaptureError::Invariant("fixture light layer missing"))?;
                    *visibility = if state.mode == ViewLightCase::Layered
                        || (state.mode == ViewLightCase::Hidden && image == 1)
                    {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    };
                    *layers = RenderLayers::layer(
                        if state.mode == ViewLightCase::Layered && image != 1 {
                            2
                        } else {
                            1
                        },
                    );
                }
            }
            if state.mode == ViewLightCase::Parented {
                let mut parent = parents
                    .get_mut(
                        state
                            .parent
                            .ok_or(LightCaptureError::Invariant("parent missing"))?,
                    )
                    .map_err(|_| LightCaptureError::Invariant("actual parent transform missing"))?;
                parent.rotation = if image == 1 {
                    Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
                } else {
                    Quat::IDENTITY
                };
            }
            state.applied = Some(image);
            request.nonce = u64::try_from(image + 1)
                .map_err(|_| LightCaptureError::Invariant("request epoch overflow"))?;
            request.views = state.cameras;
            return Ok(());
        }
        let Some(report) = evidence.read(request.nonce)? else {
            return Ok(());
        };
        if report.nonce != request.nonce {
            return Ok(());
        }
        verify_report(
            state.mode,
            image,
            pool.style.reference_illuminance_lux,
            &report,
        )?;
        let proof = gate.snapshot()?;
        if let Some(baseline) = &state.baseline {
            if proof.bytes != baseline.bytes
                || proof.targets != baseline.targets
                || proof.staging != baseline.staging
            {
                return Err(LightCaptureError::Invariant(
                    "frozen packet bytes or persistent identities changed",
                ));
            }
        } else {
            state.baseline = Some(proof.clone());
        }
        if state.mode == ViewLightCase::Stalled && image == 1 {
            let held = state
                .retained
                .as_ref()
                .ok_or(LightCaptureError::Invariant("retained packet missing"))?;
            if proof.sequence != held.sequence
                || proof.bytes != held.bytes
                || proof.held != pool.style.gpu_staging_buffers
            {
                return Err(LightCaptureError::Invariant(
                    "lighting transition did not retain the selected packet",
                ));
            }
        }
        let filename = format!("view-light-{image}.packet.bin");
        std::fs::write(options.out.join(&filename), &proof.bytes)?;
        state.checkpoints.push(Checkpoint {
            image,
            presentation_frame: progress.elapsed_frames,
            pool_seconds: pool.clock,
            packet_sequence: proof.sequence,
            packet_file: filename,
            packet_bytes: proof.bytes.len(),
            held_callbacks: proof.held,
            target_ids: format!("{:?}", proof.targets),
            staging_ids: format!("{:?}", proof.staging),
            actual_view_lights: report,
        });
        state.ready = Some(image);
        Ok(())
    })();
    if let Err(error) = result {
        state.terminal = Some(Terminal::Failed);
        error!("{error}");
        exit.send(AppExit::error());
    }
}

fn verify_report(
    mode: ViewLightCase,
    image: usize,
    reference: f32,
    report: &ProbeReport,
) -> Result<(), LightCaptureError> {
    let expected_views = if mode == ViewLightCase::Layered { 2 } else { 1 };
    if report.views.len() != expected_views {
        return Err(LightCaptureError::Invariant(
            "actual view count differs from fixture",
        ));
    }
    for (index, view) in report.views.iter().enumerate() {
        if view.queued_mist != 1 {
            return Err(LightCaptureError::Invariant(
                "actual prepared view must queue the one Close wake",
            ));
        }
        let direction = Vec3::new(1.0, 2.0, 3.0).normalize();
        let total = match mode {
            ViewLightCase::Hidden if image == 1 => 2,
            ViewLightCase::Layered => 2,
            ViewLightCase::Zero if image == 1 => 0,
            _ => 1,
        };
        let eligible = if mode == ViewLightCase::Layered {
            1
        } else {
            total
        };
        if view.reading.counts[..2] != [total, eligible] {
            return Err(LightCaptureError::Invariant(
                "actual total/eligible light counts differ",
            ));
        }
        let response = match mode {
            ViewLightCase::Hidden if image == 1 => {
                Vec3::new(30_000.0, 10_000.0, 10_000.0) / reference
            }
            ViewLightCase::Zero if image == 1 => Vec3::ZERO,
            ViewLightCase::Layered if (index == 1) != (image == 1) => {
                Vec3::new(20_000.0, 0.0, 0.0) / reference
            }
            ViewLightCase::Stalled if image > 0 => Vec3::new(10_000.0, 0.0, 0.0) / reference,
            ViewLightCase::Parented => Vec3::splat(10_000.0 / reference * direction.y),
            _ => Vec3::splat(10_000.0 / reference),
        };
        let actual = view.reading.axes[1];
        if !Vec3::new(actual[0], actual[1], actual[2]).abs_diff_eq(response, 0.00002) {
            return Err(LightCaptureError::Invariant(
                "actual view-bound RGB response differs",
            ));
        }
        if mode == ViewLightCase::Parented {
            let x = view.reading.axes[0];
            let expected = Vec3::splat(
                10_000.0 / reference * if image == 1 { direction.z } else { direction.x },
            );
            if !Vec3::new(x[0], x[1], x[2]).abs_diff_eq(expected, 0.00002) {
                return Err(LightCaptureError::Invariant(
                    "parent world rotation did not change the light direction",
                ));
            }
        }
    }
    if report.views.len() == 2 && report.views[0].offset == report.views[1].offset {
        return Err(LightCaptureError::Invariant(
            "two views reused one dynamic light offset",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::mist::capture::CaptureOptions;
    use clap::Parser;

    #[test]
    fn invalid_light_injection_requires_the_real_held_callback_fixture() -> Result<(), clap::Error>
    {
        for args in [
            vec![
                "mist_preview",
                "--out",
                "unused",
                "--verify-invalid-view-light",
            ],
            vec![
                "mist_preview",
                "--out",
                "unused",
                "--verify-view-lights",
                "hidden",
                "--verify-invalid-view-light",
            ],
            vec![
                "mist_preview",
                "--out",
                "unused",
                "--case",
                "crowd",
                "--verify-view-lights",
                "stalled",
                "--verify-invalid-view-light",
            ],
            vec![
                "mist_preview",
                "--out",
                "unused",
                "--benchmark-emitters",
                "100",
                "--verify-view-lights",
                "stalled",
                "--verify-invalid-view-light",
            ],
        ] {
            let options = CaptureOptions::try_parse_from(args)?;
            assert!(validate(&options).is_err());
        }
        let options = CaptureOptions::try_parse_from([
            "mist_preview",
            "--out",
            "unused",
            "--verify-view-lights",
            "stalled",
            "--verify-invalid-view-light",
        ])?;
        assert!(validate(&options).is_ok());
        Ok(())
    }
}
