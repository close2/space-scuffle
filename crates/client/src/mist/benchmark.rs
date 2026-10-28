//! Bounded real-time measurements for the fixed-scene mist performance comparison.
//!
//! Wall time uses the monotonic system clock independently of Bevy's manually
//! stepped presentation time. Samples allocate before measurement and never grow;
//! statistics and file formatting happen after the timed interval has ended.

use bevy::prelude::*;
use serde::Serialize;
use std::time::{Duration, Instant};

/// Maximum recorded samples per measured distribution, allocated before timing.
pub(super) const MAX_SAMPLES: usize = 65_536;

/// Explicit failures invalidate a benchmark instead of dropping measurements.
#[derive(Debug, thiserror::Error)]
pub(super) enum BenchmarkError {
    #[error("benchmark sample capacity exhausted ({MAX_SAMPLES})")]
    SampleCapacity,
    #[error("benchmark measurement must be finite and nonnegative")]
    InvalidSample,
    #[error("benchmark requires at least one measured sample")]
    EmptySamples,
    #[error("benchmark camera bounds/projection are invalid")]
    InvalidCamera,
    #[error("benchmark warmup duration/frame requirement is invalid or overflows")]
    InvalidWarmup,
}

/// Optional production mist-update instrumentation, measured in wall milliseconds.
///
/// Insert this resource only for profiling; normal game updates do not read a clock.
#[derive(Resource, Default)]
pub struct MistCpuTiming {
    /// Complete main-world mist update duration, including geometry when present.
    pub update_ms: f64,
    /// CPU ribbon geometry duration within that update; excludes GPU work.
    pub geometry_ms: f64,
}

/// Optional bounded instrumentation of one production mist update.
pub(crate) struct CpuTimer {
    started: Option<Instant>,
    geometry_ms: f64,
}

impl CpuTimer {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            started: enabled.then(Instant::now),
            geometry_ms: 0.0,
        }
    }

    pub(crate) fn geometry_start(&self) -> Option<Instant> {
        self.started.map(|_| Instant::now())
    }

    pub(crate) fn geometry_end(&mut self, started: Option<Instant>) {
        if let Some(started) = started {
            self.geometry_ms += started.elapsed().as_secs_f64() * 1000.0;
        }
    }

    pub(crate) fn finish(self, output: Option<&mut MistCpuTiming>) {
        if let (Some(started), Some(output)) = (self.started, output) {
            output.update_ms = started.elapsed().as_secs_f64() * 1000.0;
            output.geometry_ms = self.geometry_ms;
        }
    }
}

pub(super) struct Samples {
    values: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub(super) struct Distribution {
    pub samples: usize,
    pub minimum: f64,
    pub median: f64,
    pub p95: f64,
    pub p99: f64,
    pub maximum: f64,
    pub mean: f64,
}

impl Samples {
    pub(super) fn new() -> Self {
        // PERFORMANCE RATIONALE:
        // Why: Timing instrumentation must not allocate one record per frame.
        // What: Reserve a fixed upper bound before the measurement begins.
        // Trade-off: Exhaustion fails the run instead of growing or losing samples.
        Self {
            values: Vec::with_capacity(MAX_SAMPLES),
        }
    }

    pub(super) fn record(&mut self, value: f64) -> Result<(), BenchmarkError> {
        if !value.is_finite() || value < 0.0 {
            return Err(BenchmarkError::InvalidSample);
        }
        if self.values.len() == MAX_SAMPLES {
            return Err(BenchmarkError::SampleCapacity);
        }
        self.values.push(value);
        Ok(())
    }

    pub(super) fn sample_count(&self) -> usize {
        self.values.len()
    }

    pub(super) fn summarize(&mut self) -> Result<Distribution, BenchmarkError> {
        if self.values.is_empty() {
            return Err(BenchmarkError::EmptySamples);
        }
        self.values.sort_unstable_by(f64::total_cmp);
        let count = self.values.len();
        let percentile = |percent: usize| {
            // Nearest-rank quantiles, including the upper tail on short runs.
            self.values[(count * percent).div_ceil(100).saturating_sub(1)]
        };
        Ok(Distribution {
            samples: count,
            minimum: self.values[0],
            median: percentile(50),
            p95: percentile(95),
            p99: percentile(99),
            maximum: self.values[count - 1],
            mean: self.values.iter().sum::<f64>() / count as f64,
        })
    }
}

/// Minimum complete virtual frames needed to mature a history plus its final interval.
/// Floating authored seconds are converted once; all frame rounding is checked integer math.
pub(super) fn history_warmup_frames(
    lifetime_seconds: f32,
    maximum_sample_seconds: f32,
    frequency: u32,
) -> Result<u32, BenchmarkError> {
    if lifetime_seconds <= 0.0 || maximum_sample_seconds <= 0.0 || frequency == 0 {
        return Err(BenchmarkError::InvalidWarmup);
    }
    let lifetime = Duration::try_from_secs_f64(f64::from(lifetime_seconds))
        .map_err(|_| BenchmarkError::InvalidWarmup)?;
    let interval = Duration::try_from_secs_f64(f64::from(maximum_sample_seconds))
        .map_err(|_| BenchmarkError::InvalidWarmup)?;
    let duration = lifetime
        .checked_add(interval)
        .ok_or(BenchmarkError::InvalidWarmup)?;
    let frame_nanos = duration
        .as_nanos()
        .checked_mul(u128::from(frequency))
        .ok_or(BenchmarkError::InvalidWarmup)?;
    u32::try_from(frame_nanos.div_ceil(1_000_000_000)).map_err(|_| BenchmarkError::InvalidWarmup)
}

/// Conservative world bounds for the fixed crowd fixture, including full wake support.
/// Origin vectors contain world X/Z; ground heights, radii and excursions are world units.
/// Sphere centers can be one authored radius above terrain; horizontal motion adds no Y.
pub(super) fn crowd_camera_bounds(
    minimum_origins: Vec2,
    maximum_origins: Vec2,
    minimum_ground: f32,
    maximum_ground: f32,
    maximum_radius: f32,
    maximum_wake_excursion: f32,
    horizontal_motion_extent: f32,
) -> Result<(Vec3, Vec3), BenchmarkError> {
    if !minimum_origins.is_finite()
        || !maximum_origins.is_finite()
        || minimum_origins.cmpgt(maximum_origins).any()
        || ![
            minimum_ground,
            maximum_ground,
            maximum_radius,
            maximum_wake_excursion,
            horizontal_motion_extent,
        ]
        .iter()
        .all(|value| value.is_finite())
        || minimum_ground > maximum_ground
        || maximum_radius <= 0.0
        || maximum_wake_excursion < 0.0
        || horizontal_motion_extent < 0.0
    {
        return Err(BenchmarkError::InvalidCamera);
    }
    let support = maximum_radius.max(maximum_wake_excursion);
    let horizontal_extent = horizontal_motion_extent + support;
    let minimum = Vec3::new(
        minimum_origins.x - horizontal_extent,
        minimum_ground - support,
        minimum_origins.y - horizontal_extent,
    );
    let maximum = Vec3::new(
        maximum_origins.x + horizontal_extent,
        maximum_ground + maximum_radius + support,
        maximum_origins.y + horizontal_extent,
    );
    if !minimum.is_finite() || !maximum.is_finite() {
        return Err(BenchmarkError::InvalidCamera);
    }
    Ok((minimum, maximum))
}

/// Fits a fixed perspective camera around every corner with a 5% viewport margin.
/// Bounds and returned transform are in world units; aspect is physical width/height.
pub(super) fn fit_camera(
    minimum: Vec3,
    maximum: Vec3,
    aspect: f32,
    vertical_fov: f32,
) -> Result<Transform, BenchmarkError> {
    if !minimum.is_finite()
        || !maximum.is_finite()
        || !minimum.cmplt(maximum).all()
        || !aspect.is_finite()
        || aspect <= 0.0
        || !vertical_fov.is_finite()
        || !(0.0..std::f32::consts::PI).contains(&vertical_fov)
    {
        return Err(BenchmarkError::InvalidCamera);
    }
    let center = (minimum + maximum) * 0.5;
    let back = Vec3::new(0.05, 0.8, 0.6).normalize();
    let right = Vec3::Y.cross(back).normalize();
    let up = back.cross(right);
    let tan_y = (vertical_fov * 0.5).tan() * 0.9;
    let tan_x = tan_y * aspect;
    let mut distance = 1.0_f32;
    for x in [minimum.x, maximum.x] {
        for y in [minimum.y, maximum.y] {
            for z in [minimum.z, maximum.z] {
                let offset = Vec3::new(x, y, z) - center;
                distance = distance
                    .max(offset.dot(back) + offset.dot(right).abs() / tan_x)
                    .max(offset.dot(back) + offset.dot(up).abs() / tan_y)
                    .max(offset.dot(back) + 1.0);
            }
        }
    }
    Ok(Transform::from_translation(center + back * distance).looking_at(center, Vec3::Y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_samples_preserve_upper_tail_and_reject_loss() -> Result<(), BenchmarkError> {
        let mut samples = Samples::new();
        assert!(matches!(
            samples.summarize(),
            Err(BenchmarkError::EmptySamples)
        ));
        assert!(matches!(
            samples.record(f64::NAN),
            Err(BenchmarkError::InvalidSample)
        ));
        let pointer = samples.values.as_ptr();
        for value in (1..=100_u16).rev() {
            samples.record(f64::from(value))?;
        }
        let summary = samples.summarize()?;
        assert_eq!(
            (summary.median, summary.p95, summary.p99),
            (50.0, 95.0, 99.0)
        );
        while samples.values.len() < MAX_SAMPLES {
            samples.record(1.0)?;
        }
        assert!(matches!(
            samples.record(1.0),
            Err(BenchmarkError::SampleCapacity)
        ));
        assert_eq!(samples.values.as_ptr(), pointer);
        Ok(())
    }

    #[test]
    fn warmup_frames_round_up_and_reject_invalid_or_overflowing_inputs(
    ) -> Result<(), BenchmarkError> {
        assert_eq!(history_warmup_frames(1.0, 0.25, 60)?, 75);
        assert_eq!(history_warmup_frames(1.0, 0.25, 30)?, 38);
        assert_eq!(history_warmup_frames(1.0, 0.25, 144)?, 180);
        assert!(history_warmup_frames(1.0, 0.25, 0).is_err());
        assert!(history_warmup_frames(f32::NAN, 0.25, 60).is_err());
        assert!(history_warmup_frames(f32::INFINITY, 0.25, 60).is_err());
        assert!(history_warmup_frames(-1.0, 0.25, 60).is_err());
        assert!(history_warmup_frames(f32::MAX, 0.25, 60).is_err());
        assert!(history_warmup_frames(10.0, 1.0, u32::MAX).is_err());
        Ok(())
    }

    #[test]
    fn crowd_bounds_include_large_authored_radius_above_highest_ground(
    ) -> Result<(), BenchmarkError> {
        let (minimum, maximum) = crowd_camera_bounds(
            Vec2::splat(4.0),
            Vec2::splat(51.5),
            -2.0,
            7.0,
            3.0,
            4.0,
            1.0,
        )?;
        assert_eq!(minimum, Vec3::new(-1.0, -6.0, -1.0));
        assert_eq!(maximum, Vec3::new(56.5, 14.0, 56.5));
        // Highest center is terrain + radius. Both actor and wake must fit above it.
        assert!(7.0 + 3.0 + 3.0 <= maximum.y);
        assert!(7.0 + 3.0 + 4.0 <= maximum.y);
        assert!(crowd_camera_bounds(
            Vec2::ZERO,
            Vec2::ONE,
            0.0,
            1.0,
            f32::MAX,
            f32::MAX,
            f32::MAX
        )
        .is_err());
        let transform = fit_camera(minimum, maximum, 16.0 / 9.0, std::f32::consts::FRAC_PI_4)?;
        let projected_top = transform
            .compute_matrix()
            .inverse()
            .transform_point3(Vec3::new(51.5, 14.0, 51.5));
        assert!(
            (projected_top.y / (-projected_top.z * (std::f32::consts::FRAC_PI_4 * 0.5).tan()))
                .abs()
                <= 0.900_001
        );
        Ok(())
    }

    #[test]
    fn fitted_camera_contains_extreme_bounds_at_multiple_aspects() -> Result<(), BenchmarkError> {
        let minimum = Vec3::new(-7.0, -2.0, -7.0);
        let maximum = Vec3::new(62.0, 17.0, 62.0);
        for aspect in [1.0, 16.0 / 9.0, 32.0 / 9.0] {
            let fov = std::f32::consts::FRAC_PI_4;
            let transform = fit_camera(minimum, maximum, aspect, fov)?;
            let inverse = transform.compute_matrix().inverse();
            for x in [minimum.x, maximum.x] {
                for y in [minimum.y, maximum.y] {
                    for z in [minimum.z, maximum.z] {
                        let camera = inverse.transform_point3(Vec3::new(x, y, z));
                        assert!(camera.z < 0.0);
                        assert!(
                            (camera.x / (-camera.z * (fov * 0.5).tan() * aspect)).abs()
                                <= 0.900_001
                        );
                        assert!((camera.y / (-camera.z * (fov * 0.5).tan())).abs() <= 0.900_001);
                    }
                }
            }
        }
        assert!(fit_camera(minimum, maximum, 0.0, 1.0).is_err());
        Ok(())
    }
}

/// Explicit public-schedule work owned by the mist transfer, excluding driver internals.
#[derive(Clone, Copy)]
pub(super) enum TransferStage {
    Extraction,
    Selection,
    CopyRecording,
    Acknowledgment,
}

/// Independent distributions; these CPU times are never presented as GPU upload time.
#[derive(Serialize)]
pub(super) struct TransferReport {
    pub unit: &'static str,
    pub extraction_and_packet_packing_ms: Distribution,
    pub selection_and_mapped_copy_ms: Distribution,
    pub graph_copy_command_recording_ms: Distribution,
    pub post_submission_acknowledgment_ms: Distribution,
}

pub(super) struct TransferMeasurements {
    active: bool,
    extraction: Samples,
    selection: Samples,
    copy_recording: Samples,
    acknowledgment: Samples,
}

impl TransferMeasurements {
    pub fn new() -> Self {
        Self {
            active: false,
            extraction: Samples::new(),
            selection: Samples::new(),
            copy_recording: Samples::new(),
            acknowledgment: Samples::new(),
        }
    }

    pub fn start(&mut self) {
        self.extraction.values.clear();
        self.selection.values.clear();
        self.copy_recording.values.clear();
        self.acknowledgment.values.clear();
        self.active = true;
    }

    pub fn record(
        &mut self,
        stage: TransferStage,
        elapsed: Duration,
    ) -> Result<(), BenchmarkError> {
        if !self.active {
            return Ok(());
        }
        let samples = match stage {
            TransferStage::Extraction => &mut self.extraction,
            TransferStage::Selection => &mut self.selection,
            TransferStage::CopyRecording => &mut self.copy_recording,
            TransferStage::Acknowledgment => &mut self.acknowledgment,
        };
        samples.record(elapsed.as_secs_f64() * 1000.0)
    }

    pub fn finish(&mut self) -> Result<TransferReport, BenchmarkError> {
        self.active = false;
        Ok(TransferReport {
            unit: "milliseconds",
            extraction_and_packet_packing_ms: self.extraction.summarize()?,
            selection_and_mapped_copy_ms: self.selection.summarize()?,
            graph_copy_command_recording_ms: self.copy_recording.summarize()?,
            post_submission_acknowledgment_ms: self.acknowledgment.summarize()?,
        })
    }
}
