//! Real-render benchmark orchestration for the existing capture fixture.
//!
//! Keeps 400 actual actors and all graphics settings fixed while enabling a chosen
//! number of emitters. Diagnostics allocate during warmup; timed observations use
//! bounded samples, and screenshots begin only after measurement and reporting.

use super::{CaptureOptions, CaptureProgress};
use crate::mist::benchmark::{BenchmarkError, Distribution, MistCpuTiming, Samples};
use crate::mist::MistPool;
use bevy::diagnostic::{DiagnosticPath, DiagnosticsStore};
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::mesh::{Indices, RenderMesh};
use bevy::render::primitives::Aabb;
use bevy::render::render_asset::ExtractedAssets;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSet};
use bevy::utils::Instant as DiagnosticInstant;
use serde::Serialize;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WARMUP_SECONDS: u64 = 3;
const MAX_PASS_METRICS: usize = 128;
const MAX_WALL_SECONDS: u64 = 180;

#[derive(Debug)]
pub(super) struct FrustumEvidence {
    pub actors: usize,
    pub active_ribbons: usize,
}

pub(super) fn verify_bounds(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    minimum: Vec3,
    maximum: Vec3,
    world_from_local: Mat4,
) -> Result<(), BenchmarkError> {
    for x in [minimum.x, maximum.x] {
        for y in [minimum.y, maximum.y] {
            for z in [minimum.z, maximum.z] {
                let point = world_from_local.transform_point3(Vec3::new(x, y, z));
                let projected = camera
                    .world_to_ndc(camera_transform, point)
                    .ok_or(BenchmarkError::InvalidCamera)?;
                if !projected.is_finite()
                    || projected.x.abs() > 0.901
                    || projected.y.abs() > 0.901
                    || !(0.0..=1.0).contains(&projected.z)
                {
                    return Err(BenchmarkError::InvalidCamera);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn verify_ribbon_bounds(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    bounds: &Aabb,
    transform: &GlobalTransform,
) -> Result<(), BenchmarkError> {
    verify_bounds(
        camera,
        camera_transform,
        (bounds.center - bounds.half_extents).into(),
        (bounds.center + bounds.half_extents).into(),
        transform.compute_matrix(),
    )
}

#[derive(Default, Clone, Copy, Serialize)]
struct ExtractionCounts {
    render_frames: u64,
    mesh_assets: u64,
    geometry_bytes: u64,
    probe_cpu_ms: f64,
}

#[derive(Default)]
struct ExtractionState {
    active: bool,
    counts: ExtractionCounts,
}

#[derive(Resource)]
struct ExtractionProbe {
    ids: Vec<AssetId<Mesh>>,
    state: Arc<Mutex<ExtractionState>>,
    failed: Arc<AtomicBool>,
}

struct PassMetric {
    path: DiagnosticPath,
    unit: String,
    last: Option<DiagnosticInstant>,
    samples: Samples,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Loading,
    Warmup(Instant),
    Measuring { started: Instant, previous: Instant },
    Complete,
    Failed,
}

#[derive(Debug, Clone, Copy)]
struct ObservedSettings {
    resolution: [u32; 2],
    present_mode: bevy::window::PresentMode,
    msaa: u32,
}

#[derive(Debug, thiserror::Error)]
enum ReadinessError {
    #[error("benchmark wall deadline expired after {elapsed:?}; phase={phase:?}, actual settings={observed:?}")]
    Deadline {
        elapsed: Duration,
        phase: Phase,
        observed: ObservedSettings,
    },
    #[error("benchmark settings mismatch in phase={phase:?}: actual={observed:?}; expected physical=[1280, 720], presentation=AutoNoVsync, MSAA={expected_msaa}")]
    Settings {
        phase: Phase,
        observed: ObservedSettings,
        expected_msaa: u32,
    },
}

/// Window negotiation may defer readiness, but cannot relax the measured contract.
fn settings_ready(
    phase: Phase,
    elapsed: Duration,
    observed: ObservedSettings,
    expected_msaa: u32,
) -> Result<bool, ReadinessError> {
    if elapsed > Duration::from_secs(MAX_WALL_SECONDS) {
        return Err(ReadinessError::Deadline {
            elapsed,
            phase,
            observed,
        });
    }
    let resolution_matches = observed.resolution == [1280, 720];
    if observed.present_mode != bevy::window::PresentMode::AutoNoVsync
        || observed.msaa != expected_msaa
        || (!resolution_matches && !matches!(phase, Phase::Loading))
    {
        return Err(ReadinessError::Settings {
            phase,
            observed,
            expected_msaa,
        });
    }
    Ok(resolution_matches)
}

#[derive(Resource)]
pub(super) struct Recorder {
    phase: Phase,
    deadline: Instant,
    wall: Samples,
    update: Samples,
    geometry: Samples,
    passes: Vec<PassMetric>,
    extraction: Arc<Mutex<ExtractionState>>,
    extraction_failed: Arc<AtomicBool>,
    start_load: String,
    resident_kib: Vec<u64>,
    last_rss: Instant,
    warmup_wall_seconds: f64,
    warmup_virtual_frames: u32,
    history_ready_frames: u32,
    gpu_start: super::super::gpu::GpuAccounting,
    mesh_ids: Vec<AssetId<Mesh>>,
    mesh_modified_events: u64,
}

impl Recorder {
    pub(super) fn complete(&self) -> bool {
        matches!(self.phase, Phase::Complete)
    }
}

pub(super) fn install(app: &mut App) {
    let extraction = Arc::new(Mutex::new(ExtractionState::default()));
    let extraction_failed = Arc::new(AtomicBool::new(false));
    app.insert_resource(Recorder {
        phase: Phase::Loading,
        deadline: Instant::now(),
        wall: Samples::new(),
        update: Samples::new(),
        geometry: Samples::new(),
        passes: Vec::with_capacity(MAX_PASS_METRICS),
        extraction: extraction.clone(),
        extraction_failed: extraction_failed.clone(),
        start_load: String::new(),
        resident_kib: Vec::with_capacity(64),
        last_rss: Instant::now(),
        warmup_wall_seconds: 0.0,
        warmup_virtual_frames: 0,
        history_ready_frames: 0,
        gpu_start: super::super::gpu::GpuAccounting::default(),
        mesh_ids: Vec::new(),
        mesh_modified_events: 0,
    })
    .init_resource::<MistCpuTiming>()
    .add_plugins(RenderDiagnosticsPlugin)
    .add_systems(
        Last,
        (
            observe_mesh_events.before(observe),
            observe.before(super::capture_frames),
        ),
    );
    app.sub_app_mut(RenderApp)
        .insert_resource(ExtractionProbe {
            ids: Vec::new(),
            state: extraction,
            failed: extraction_failed,
        })
        .add_systems(ExtractSchedule, initialize_probe)
        .add_systems(
            Render,
            count_extracted
                .after(RenderSet::ExtractCommands)
                .before(RenderSet::PrepareAssets),
        );
}

fn observe_mesh_events(
    mut recorder: ResMut<'_, Recorder>,
    pool: Option<Res<'_, MistPool>>,
    mut events: EventReader<'_, '_, AssetEvent<Mesh>>,
) {
    let Some(pool) = pool else { return };
    if recorder.mesh_ids.is_empty() {
        recorder.mesh_ids.reserve_exact(pool.slots.len());
        recorder
            .mesh_ids
            .extend(pool.slots.iter().map(|slot| slot.mesh.id()));
        recorder.mesh_ids.sort_unstable();
    }
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            if matches!(recorder.phase, Phase::Measuring { .. })
                && recorder.mesh_ids.binary_search(id).is_ok()
            {
                let Some(count) = recorder.mesh_modified_events.checked_add(1) else {
                    recorder.phase = Phase::Failed;
                    error!("mist mesh modification evidence counter exhausted");
                    return;
                };
                recorder.mesh_modified_events = count;
            }
        }
    }
}

fn initialize_probe(
    mut probe: ResMut<'_, ExtractionProbe>,
    pool: Extract<'_, '_, Option<Res<'_, MistPool>>>,
) {
    if !probe.ids.is_empty() {
        return;
    }
    if let Some(pool) = pool.as_ref() {
        // Initialization only: the pool cannot change capacity during a run.
        probe.ids.reserve_exact(pool.slots.len());
        probe
            .ids
            .extend(pool.slots.iter().map(|slot| slot.mesh.id()));
        probe.ids.sort_unstable();
    }
}

fn count_extracted(probe: Res<'_, ExtractionProbe>, assets: Res<'_, ExtractedAssets<RenderMesh>>) {
    let Ok(mut state) = probe.state.lock() else {
        probe.failed.store(true, Ordering::Release);
        return;
    };
    if !state.active {
        return;
    }
    let started = Instant::now();
    state.counts.render_frames += 1;
    for (id, mesh) in &assets.extracted {
        if probe.ids.binary_search(id).is_ok() {
            let index_bytes = match mesh.indices() {
                Some(Indices::U16(indices)) => indices.len() * size_of::<u16>(),
                Some(Indices::U32(indices)) => indices.len() * size_of::<u32>(),
                None => 0,
            };
            state.counts.mesh_assets += 1;
            state.counts.geometry_bytes += (mesh.get_vertex_buffer_size() + index_bytes) as u64;
        }
    }
    state.counts.probe_cpu_ms += started.elapsed().as_secs_f64() * 1000.0;
}

#[derive(Serialize)]
struct PassReport {
    path: String,
    unit: String,
    distribution: Option<Distribution>,
}

#[derive(Serialize)]
struct MediaUse {
    unit_type: sim_types::UnitTypeId,
    action: sim_types::AnimState,
    actor_count: usize,
    admitted: asset_baker::media_audit::MediaAuditRecord,
}

#[derive(Serialize)]
struct Report {
    format_version: u32,
    implementation: &'static str,
    actors: usize,
    actor_media: Vec<MediaUse>,
    emitters: u16,
    requested_virtual_hz: u32,
    wall_seconds: f64,
    warmup_wall_seconds: f64,
    warmup_virtual_frames: u32,
    history_ready_frames: u32,
    measured_frames_per_second: f64,
    physical_resolution: [u32; 2],
    msaa: u32,
    presentation_mode: String,
    audio_policy: crate::presentation::AudioPolicy,
    adapter: String,
    device_features: String,
    wall_frame_ms: Distribution,
    main_mist_update_ms: Distribution,
    main_ribbon_geometry_ms: Distribution,
    render_passes: Vec<PassReport>,
    mist_mesh_modified_events: u64,
    extracted_mist_meshes: ExtractionCounts,
    transfer_cpu: crate::mist::benchmark::TransferReport,
    application_gpu_buffers_at_end: super::super::gpu::GpuAccounting,
    application_gpu_changes_during_measurement: super::super::gpu::GpuAccounting,
    application_gpu_upload_bytes_per_wall_second: f64,
    extracted_geometry_bytes_per_wall_second: f64,
    private_extraction_upload_cpu_timings: &'static str,
    gpu_timing_evidence: &'static str,
    load_before: String,
    load_after: String,
    resident_kib_samples: Vec<u64>,
    resident_memory_evidence: &'static str,
    allocation_boundary: &'static str,
}

#[allow(clippy::too_many_arguments)]
fn observe(
    mut recorder: ResMut<'_, Recorder>,
    progress: Res<'_, CaptureProgress>,
    pool: Res<'_, MistPool>,
    bubbles: Query<'_, '_, &crate::BubbleInstance, With<super::CaptureBubble>>,
    media: Res<'_, crate::media::ClientMediaStatus>,
    options: Res<'_, CaptureOptions>,
    timings: Res<'_, MistCpuTiming>,
    diagnostics: Res<'_, DiagnosticsStore>,
    adapter: Res<'_, RenderAdapterInfo>,
    device: Res<'_, RenderDevice>,
    policy: Res<'_, crate::presentation::AudioPolicy>,
    windows: Query<'_, '_, &Window>,
    cameras: Query<'_, '_, &Msaa, With<Camera3d>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if recorder.complete() || matches!(recorder.phase, Phase::Failed) {
        return;
    }
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        if recorder.extraction_failed.load(Ordering::Acquire) {
            return Err("render extraction probe failed; measurements are incomplete".into());
        }
        {
            let mut publication = pool
                .publication
                .lock()
                .map_err(|_| "GPU publication mutex poisoned")?;
            if let Some(error) = &publication.error {
                return Err(error.clone().into());
            }
            if matches!(recorder.phase, Phase::Loading) && publication.measurements.is_none() {
                publication.measurements =
                    Some(crate::mist::benchmark::TransferMeasurements::new());
            }
        }
        let window = windows.get_single()?;
        let resolution = [window.physical_width(), window.physical_height()];
        let msaa = cameras.get_single()?.samples();
        if !settings_ready(
            recorder.phase,
            recorder.deadline.elapsed(),
            ObservedSettings {
                resolution,
                present_mode: window.present_mode,
                msaa,
            },
            options.msaa,
        )? {
            return Ok(());
        }
        let now = Instant::now();
        match recorder.phase {
            Phase::Loading if progress.running => recorder.phase = Phase::Warmup(now),
            Phase::Warmup(started) if started.elapsed() >= Duration::from_secs(WARMUP_SECONDS) => {
                let required = crate::mist::benchmark::history_warmup_frames(
                    pool.style.lifetime_seconds,
                    pool.style.max_sample_seconds,
                    options.fps,
                )?;
                if progress.elapsed_frames < required {
                    return Ok(());
                }
                recorder.warmup_wall_seconds = started.elapsed().as_secs_f64();
                recorder.warmup_virtual_frames = progress.elapsed_frames;
                recorder.history_ready_frames = required;
                for diagnostic in diagnostics
                    .iter()
                    .filter(|d| d.path().as_str().starts_with("render/"))
                {
                    if recorder.passes.len() == MAX_PASS_METRICS {
                        return Err("render diagnostics exceeded fixed metric capacity".into());
                    }
                    recorder.passes.push(PassMetric {
                        path: diagnostic.path().clone(),
                        unit: diagnostic.suffix.to_string(),
                        last: diagnostic.measurement().map(|m| m.time),
                        samples: Samples::new(),
                    });
                }
                if recorder.passes.is_empty() {
                    return Err("real renderer has not published pass diagnostics".into());
                }
                recorder.mesh_modified_events = 0;
                recorder.start_load = system_load();
                if let Some(rss) = resident_kib() {
                    recorder.resident_kib.push(rss);
                }
                {
                    let mut shared = recorder
                        .extraction
                        .lock()
                        .map_err(|_| "extraction probe mutex poisoned")?;
                    shared.counts = ExtractionCounts::default();
                    shared.active = true;
                }
                {
                    let mut publication = pool
                        .publication
                        .lock()
                        .map_err(|_| "GPU publication mutex poisoned")?;
                    recorder.gpu_start = publication.accounting;
                    publication
                        .measurements
                        .as_mut()
                        .ok_or("transfer measurements absent")?
                        .start();
                }
                let started = Instant::now();
                recorder.last_rss = started;
                recorder.phase = Phase::Measuring {
                    started,
                    previous: started,
                };
            }
            Phase::Measuring { started, previous } => {
                recorder
                    .wall
                    .record(now.duration_since(previous).as_secs_f64() * 1000.0)?;
                recorder.update.record(timings.update_ms)?;
                recorder.geometry.record(timings.geometry_ms)?;
                for pass in &mut recorder.passes {
                    if let Some(measurement) =
                        diagnostics.get(&pass.path).and_then(|d| d.measurement())
                    {
                        if pass.last != Some(measurement.time) {
                            pass.samples.record(measurement.value)?;
                            pass.last = Some(measurement.time);
                        }
                    }
                }
                if recorder.last_rss.elapsed() >= Duration::from_secs(1) {
                    if let Some(rss) = resident_kib() {
                        if recorder.resident_kib.len() == 64 {
                            return Err("RSS sample capacity exhausted".into());
                        }
                        recorder.resident_kib.push(rss);
                    }
                    recorder.last_rss = now;
                }
                let elapsed = now.duration_since(started);
                if elapsed >= Duration::from_secs(u64::from(options.benchmark_seconds)) {
                    let counts = {
                        let mut shared = recorder
                            .extraction
                            .lock()
                            .map_err(|_| "extraction probe mutex poisoned")?;
                        shared.active = false;
                        shared.counts
                    };
                    let (gpu_end, transfer_cpu) = {
                        let mut publication = pool
                            .publication
                            .lock()
                            .map_err(|_| "GPU publication mutex poisoned")?;
                        (
                            publication.accounting,
                            publication
                                .measurements
                                .as_mut()
                                .ok_or("transfer measurements absent")?
                                .finish()?,
                        )
                    };
                    // The timed interval has ended. Group real fixture bindings and reuse
                    // byte-level admission evidence; no timed I/O or hashing is introduced.
                    let mut actor_counts = std::collections::BTreeMap::new();
                    for bubble in &bubbles {
                        *actor_counts
                            .entry((bubble.0.unit_type, bubble.0.anim_state))
                            .or_insert(0_usize) += 1;
                    }
                    let actors: usize = actor_counts.values().sum();
                    if actors != 400 {
                        return Err(
                            "fixed-scene benchmark no longer contains exactly 400 actors".into(),
                        );
                    }
                    let actor_media = actor_counts
                        .into_iter()
                        .map(|((unit_type, action), actor_count)| {
                            let admitted = media
                                .audit(unit_type)
                                .ok_or("actual benchmark actor has no admitted media audit")?;
                            if admitted.png_digest.is_none() || admitted.metadata_digest.is_none() {
                                return Err(
                                    "actual benchmark actor media has no admitted byte digests",
                                );
                            }
                            Ok(MediaUse {
                                unit_type,
                                action,
                                actor_count,
                                admitted: admitted.clone(),
                            })
                        })
                        .collect::<Result<Vec<_>, &str>>()?;
                    let wall = recorder.wall.summarize()?;
                    let passes = recorder
                        .passes
                        .iter_mut()
                        .map(|pass| {
                            Ok(PassReport {
                                path: pass.path.to_string(),
                                unit: pass.unit.clone(),
                                distribution: if pass.samples.sample_count() == 0 {
                                    None
                                } else {
                                    Some(pass.samples.summarize()?)
                                },
                            })
                        })
                        .collect::<Result<Vec<_>, BenchmarkError>>()?;
                    if passes.iter().all(|pass| pass.distribution.is_none()) {
                        return Err(
                            "renderer published no fresh diagnostic measurements during benchmark"
                                .into(),
                        );
                    }
                    let gpu_timing = passes
                        .iter()
                        .any(|p| p.path.ends_with("elapsed_gpu") && p.distribution.is_some());
                    let gpu_delta = gpu_end.checked_delta(recorder.gpu_start)?;
                    if gpu_delta.target_buffers != 0 || gpu_delta.staging_buffers != 0 {
                        return Err("mist allocated GPU buffers during measurement".into());
                    }
                    if counts.mesh_assets != 0 || recorder.mesh_modified_events != 0 {
                        return Err(
                            "immutable mist topology was extracted during measurement".into()
                        );
                    }
                    let report = Report {
                        format_version: 2,
                        implementation: "persistent_gpu_storage",
                        actors,
                        actor_media,
                        emitters: options.benchmark_emitters.ok_or("benchmark emitter count absent")?,
                        requested_virtual_hz: options.fps,
                        wall_seconds: elapsed.as_secs_f64(),
                        warmup_wall_seconds: recorder.warmup_wall_seconds,
                        warmup_virtual_frames: recorder.warmup_virtual_frames,
                        history_ready_frames: recorder.history_ready_frames,
                        measured_frames_per_second: wall.samples as f64 / elapsed.as_secs_f64(),
                        physical_resolution: resolution,
                        msaa,
                        presentation_mode: format!("{:?}", window.present_mode),
                        audio_policy: *policy,
                        adapter: format!("{:?}", **adapter),
                        device_features: format!("{:?}", device.features()),
                        wall_frame_ms: wall,
                        main_mist_update_ms: recorder.update.summarize()?,
                        main_ribbon_geometry_ms: recorder.geometry.summarize()?,
                        render_passes: passes,
                        mist_mesh_modified_events: recorder.mesh_modified_events,
                        extracted_mist_meshes: counts,
                        transfer_cpu,
                        application_gpu_buffers_at_end: gpu_end,
                        application_gpu_changes_during_measurement: gpu_delta,
                        application_gpu_upload_bytes_per_wall_second: gpu_delta.submitted_bytes as f64 / elapsed.as_secs_f64(),
                        extracted_geometry_bytes_per_wall_second: counts.geometry_bytes as f64 / elapsed.as_secs_f64(),
                        private_extraction_upload_cpu_timings: "unavailable: locked engine systems are private; schedule latency is not substituted",
                        gpu_timing_evidence: if gpu_timing { "actual renderer timestamp diagnostics" } else { "unavailable: no GPU timestamp measurements published" },
                        load_before: recorder.start_load.clone(),
                        load_after: system_load(),
                        resident_kib_samples: recorder.resident_kib.clone(),
                        resident_memory_evidence: if recorder.resident_kib.is_empty() { "unavailable: /proc/self/status VmRSS could not be read" } else { "actual /proc/self/status VmRSS samples at one-second intervals" },
                        allocation_boundary: "application-owned persistent target/staging buffer creation and submitted raw packet bytes are counted; immutable topology has no timed extraction; engine/driver/internal map-view and diagnostic allocations are not measured or claimed bounded",
                    };
                    publish_report(
                        &mut recorder.phase,
                        &options.out.join("benchmark.ron"),
                        &ron::ser::to_string_pretty(&report, ron::ser::PrettyConfig::default())?,
                    )?;
                    info!(
                        "BENCHMARK COMPLETE: measured wall p95={}ms, {} real frames/s",
                        report.wall_frame_ms.p95, report.measured_frames_per_second
                    );
                } else {
                    recorder.phase = Phase::Measuring {
                        started,
                        previous: now,
                    };
                }
            }
            _ => {}
        }
        Ok(())
    })();
    if let Err(error) = result {
        recorder.phase = Phase::Failed;
        error!("Benchmark failed: {error}");
        exit.send(AppExit::error());
    }
}

fn system_load() -> String {
    std::fs::read_to_string("/proc/loadavg").unwrap_or_else(|error| format!("unavailable: {error}"))
}

fn resident_kib() -> Option<u64> {
    // Stack storage avoids periodic heap allocation in the measurement interval.
    let mut file = std::fs::File::open("/proc/self/status").ok()?;
    let mut buffer = [0_u8; 8192];
    let count = file.read(&mut buffer).ok()?;
    let text = std::str::from_utf8(&buffer[..count]).ok()?;
    text.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")
            .and_then(|value| value.split_whitespace().next()?.parse().ok())
    })
}

/// Publication is the final acknowledgment: a failed write cannot release screenshots.
fn publish_report(phase: &mut Phase, path: &std::path::Path, text: &str) -> std::io::Result<()> {
    *phase = Phase::Failed;
    std::fs::write(path, text)?;
    *phase = Phase::Complete;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_negotiation_is_bounded_and_cannot_relax_measurement_settings(
    ) -> Result<(), ReadinessError> {
        let temporary = ObservedSettings {
            resolution: [2040, 1147],
            present_mode: bevy::window::PresentMode::AutoNoVsync,
            msaa: 4,
        };
        assert!(!settings_ready(
            Phase::Loading,
            Duration::ZERO,
            temporary,
            4
        )?);
        assert!(matches!(
            settings_ready(Phase::Loading, Duration::from_secs(181), temporary, 4),
            Err(ReadinessError::Deadline { .. })
        ));
        let correct = ObservedSettings {
            resolution: [1280, 720],
            ..temporary
        };
        assert!(settings_ready(
            Phase::Loading,
            Duration::from_secs(1),
            correct,
            4
        )?);
        let now = Instant::now();
        for phase in [
            Phase::Warmup(now),
            Phase::Measuring {
                started: now,
                previous: now,
            },
        ] {
            let failure = settings_ready(phase, Duration::from_secs(4), temporary, 4);
            assert!(matches!(failure, Err(ReadinessError::Settings { .. })));
            if let Err(error) = failure {
                assert!(error.to_string().contains("2040"));
                assert!(error.to_string().contains("1147"));
            }
        }
        assert!(settings_ready(Phase::Loading, Duration::ZERO, correct, 1).is_err());
        assert!(settings_ready(
            Phase::Loading,
            Duration::ZERO,
            ObservedSettings {
                present_mode: bevy::window::PresentMode::AutoVsync,
                ..correct
            },
            4
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn failed_report_publication_cannot_release_capture() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let now = Instant::now();
        let mut phase = Phase::Measuring {
            started: now,
            previous: now,
        };
        assert!(publish_report(&mut phase, directory.path(), "report").is_err());
        assert!(matches!(phase, Phase::Failed));
        let path = directory.path().join("benchmark.ron");
        publish_report(&mut phase, &path, "actual report contents")?;
        assert!(matches!(phase, Phase::Complete));
        assert_eq!(std::fs::read_to_string(path)?, "actual report contents");
        Ok(())
    }
}
