//! Bounded movement history sampled by distance and elapsed presentation time.
//!
//! The simulation is never modified. Every emitter owns its clock and sampling residual;
//! equal straight-line trajectories retain equal samples independently of render frequency.

use super::{MistError, MistStyle};
use bevy::prelude::Vec3;

const EMISSION_OFFSET_RADII: f32 = 0.7;

/// Maximum world-space displacement of a sample behind its observed trajectory.
/// Any axis may receive this support on a sloped path; radius is in world units.
pub(super) fn maximum_origin_offset(radius: f32) -> f32 {
    radius * EMISSION_OFFSET_RADII
}

/// One historical point in a movement wake, in world space and presentation seconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrailSample {
    /// World-space point behind the bubble surface at emission time.
    pub position: Vec3,
    /// Emitter-local presentation time at emission, in seconds.
    pub birth: f64,
    /// Cumulative path length in world units, for coherent shader coordinates.
    pub distance: f32,
}

/// Fixed-capacity ring retaining an emitter's recent path without hot-loop allocation.
#[derive(Debug)]
pub struct EmitterHistory {
    samples: Vec<TrailSample>,
    first: usize,
    len: usize,
    previous: Option<Vec3>,
    residual: f64,
    distance: f32,
    clock: f64,
}

impl EmitterHistory {
    /// Allocates the validated style's bounded history once.
    ///
    /// # Errors
    /// Returns [`MistError`] for an invalid style.
    pub fn new(style: &MistStyle) -> Result<Self, MistError> {
        style.validate()?;
        Ok(Self {
            samples: vec![TrailSample::default(); style.samples_per_emitter],
            first: 0,
            len: 0,
            previous: None,
            residual: 0.0,
            distance: 0.0,
            clock: 0.0,
        })
    }

    /// Clears all ownership-local history while preserving the allocated buffer.
    pub fn reset(&mut self) {
        self.first = 0;
        self.len = 0;
        self.previous = None;
        self.residual = 0.0;
        self.distance = 0.0;
        self.clock = 0.0;
    }

    /// Number of retained samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether this emitter has no retained trail.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Fixed storage capacity in path samples.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.samples.capacity()
    }

    /// Current emitter-local presentation time, in seconds.
    #[must_use]
    pub fn clock(&self) -> f64 {
        self.clock
    }

    /// Returns a chronological sample, oldest first.
    #[must_use]
    pub fn sample(&self, index: usize) -> Option<TrailSample> {
        (index < self.len).then(|| self.samples[(self.first + index) % self.samples.len()])
    }

    fn push(&mut self, sample: TrailSample) {
        if self.len == self.samples.len() {
            self.samples[self.first] = sample;
            self.first = (self.first + 1) % self.samples.len();
        } else {
            let index = (self.first + self.len) % self.samples.len();
            self.samples[index] = sample;
            self.len += 1;
        }
    }

    /// Ages existing samples without emitting, for removed owners or disabled wakes.
    ///
    /// # Errors
    /// Returns [`MistError`] for negative/non-finite elapsed time.
    pub fn age(&mut self, dt: f64, style: &MistStyle) -> Result<(), MistError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(MistError::InvalidMotion);
        }
        self.clock += dt;
        while self.len > 0
            && self.clock - self.samples[self.first].birth >= f64::from(style.lifetime_seconds)
        {
            self.first = (self.first + 1) % self.samples.len();
            self.len -= 1;
        }
        Ok(())
    }

    /// Samples a rendered world-space trajectory over `dt` seconds.
    ///
    /// Samples lie behind the observed position by 0.7 rendered radii. Large position
    /// discontinuities reset history. Catch-up cost never exceeds the allocated capacity.
    ///
    /// # Errors
    /// Returns [`MistError`] for invalid coordinates, elapsed time, or radius.
    pub fn advance(
        &mut self,
        position: Vec3,
        radius: f32,
        dt: f64,
        style: &MistStyle,
    ) -> Result<(), MistError> {
        if !position.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Err(MistError::InvalidMotion);
        }
        self.age(dt, style)?;
        let Some(previous) = self.previous.replace(position) else {
            return Ok(());
        };
        let delta = position - previous;
        let length = delta.length();
        if length > radius * style.continuity_radii {
            self.first = 0;
            self.len = 0;
            self.residual = 0.0;
            return Ok(());
        }
        if dt == 0.0 {
            return Ok(());
        }
        if length <= 1.0e-6 {
            self.residual = 0.0;
            return Ok(());
        }
        let phase = (f64::from(length / (radius * style.spacing_radii)))
            .max(dt / f64::from(style.max_sample_seconds));
        let total = self.residual + phase;
        let count = (total + 1.0e-6).floor() as usize;
        // PERFORMANCE RATIONALE:
        // Why: A long presentation frame must not enqueue thousands of expired samples.
        // What: Skip directly to the newest events that fit the preallocated ring.
        // Trade-off: Oldest invisible history is discarded, preserving current spacing/residual.
        let skip = count.saturating_sub(self.samples.len());
        for event in skip..count {
            let fraction = ((event as f64 + 1.0 - self.residual) / phase).clamp(0.0, 1.0);
            self.push(TrailSample {
                position: previous.lerp(position, fraction as f32)
                    - delta / length * radius * EMISSION_OFFSET_RADII,
                birth: self.clock - dt + dt * fraction,
                distance: self.distance + length * fraction as f32,
            });
        }
        self.distance += length;
        self.residual = (total - count as f64).clamp(0.0, 1.0);
        self.age(0.0, style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitted_sample_displacement_matches_camera_support() -> Result<(), MistError> {
        let style = MistStyle::default();
        let mut history = EmitterHistory::new(&style)?;
        let radius = 3.0;
        let velocity = Vec3::new(1.0, 2.0, 2.0);
        history.advance(Vec3::ZERO, radius, 0.0, &style)?;
        history.advance(velocity, radius, 1.0, &style)?;
        assert!(!history.is_empty());
        for index in 0..history.len() {
            if let Some(sample) = history.sample(index) {
                let observed_at_birth = velocity * sample.birth as f32;
                let displacement = observed_at_birth - sample.position;
                assert!((displacement.length() - maximum_origin_offset(radius)).abs() < 0.000_002);
                assert!(displacement.y > 0.0);
            }
        }
        Ok(())
    }

    #[test]
    fn invalid_motion_is_explicit() -> Result<(), MistError> {
        let style = MistStyle::default();
        let mut history = EmitterHistory::new(&style)?;
        assert!(history.advance(Vec3::NAN, 1.0, 0.1, &style).is_err());
        assert!(history.advance(Vec3::ZERO, 0.0, 0.1, &style).is_err());
        assert!(history.age(f64::INFINITY, &style).is_err());
        Ok(())
    }
}
