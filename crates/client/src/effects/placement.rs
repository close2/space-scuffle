//! Deterministic randomness and placements of layer instances around an event (plan §5.2,
//! §5.6 "Determinism of the picture", D-12).
//!
//! # Responsibility
//! Owns the stateless hash every layer's randomness comes from ([`hash3`], [`unit_float`]) and
//! the placements: how many instances a placement makes ([`instance_count`]) and where each
//! one is and when it starts ([`place`]).
//!
//! # How it fits
//! The runtime evaluates placements every frame from the effect's seed, so nothing is stored
//! per instance and a capture of the same event stream is the same on every run of a driver.
//!
//! # Invariants
//! - No RNG state and no clock: results depend only on the arguments.
//! - At most [`MAX_INSTANCES_PER_PLACEMENT`] instances per placement.

use super::binding::{resolve_value, BoundValues};
use super::catalog::schema::PlacementDef;
use bevy::math::Vec3;

/// D-12: the most instances one placement replicates.
pub const MAX_INSTANCES_PER_PLACEMENT: u32 = 32;

/// Mixes three words into one well-distributed word (a variant of the `lowbias32` finaliser).
#[must_use]
pub const fn hash3(a: u32, b: u32, c: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.rotate_left(13) ^ c.wrapping_mul(0x85EB_CA77);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// The top 24 bits of `hash` as a float in `[0, 1)`.
#[must_use]
pub fn unit_float(hash: u32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a 24-bit integer is exact in f32"
    )]
    let mantissa = (hash >> 8) as f32;
    mantissa / 16_777_216.0
}

/// The line an event lies on, in Bevy world space (Y up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldLine {
    /// Start of the line.
    pub from: Vec3,
    /// End of the line.
    pub to: Vec3,
}

/// One placed instance: offset from the effect's anchor and extra start delay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    /// Offset from the anchor in world units.
    pub offset: Vec3,
    /// Delay in seconds added to the layer's start.
    pub delay: f32,
}

/// How many instances `placement` makes for an effect bound to `bound` on `line`.
#[must_use]
pub fn instance_count(
    placement: &PlacementDef,
    bound: &BoundValues,
    line: Option<WorldLine>,
) -> u32 {
    match placement {
        PlacementDef::Point => 1,
        PlacementDef::Disc { count, .. } => (*count).min(MAX_INSTANCES_PER_PLACEMENT),
        PlacementDef::Line { spacing, .. } => {
            let Some(line) = line else {
                return 1;
            };
            let spacing = resolve_value(spacing, bound);
            let length = line.from.distance(line.to);
            if spacing <= f32::EPSILON || !length.is_finite() {
                return 1;
            }
            let steps = (length / spacing).floor().min(32.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "steps is a non-negative whole number at most 32"
            )]
            let steps = steps as u32;
            (steps + 1).min(MAX_INSTANCES_PER_PLACEMENT)
        }
    }
}

/// Instance `index` of `placement`, for the effect whose seed is `seed`. A line placement
/// measures from the anchor, which is the line's start.
#[must_use]
pub fn place(
    placement: &PlacementDef,
    index: u32,
    seed: u32,
    bound: &BoundValues,
    line: Option<WorldLine>,
) -> Placed {
    match placement {
        PlacementDef::Point => Placed {
            offset: Vec3::ZERO,
            delay: 0.0,
        },
        PlacementDef::Disc { radius, .. } => {
            let radius = resolve_value(radius, bound);
            let distance = radius * unit_float(hash3(seed, index, 0x0D15_C001)).sqrt();
            let angle = std::f32::consts::TAU * unit_float(hash3(seed, index, 0x0A16_1E00));
            Placed {
                offset: Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance),
                delay: 0.0,
            }
        }
        PlacementDef::Line { spacing, travel } => {
            let Some(line) = line else {
                return Placed {
                    offset: Vec3::ZERO,
                    delay: 0.0,
                };
            };
            let direction = (line.to - line.from).normalize_or_zero();
            #[expect(clippy::cast_precision_loss, reason = "index is below 32")]
            let distance = resolve_value(spacing, bound) * index as f32;
            let delay = if *travel > f32::EPSILON {
                distance / travel
            } else {
                0.0
            };
            Placed {
                offset: direction * distance,
                delay,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::catalog::schema::{Binding, ValueDef};

    fn constant(value: f32) -> ValueDef {
        ValueDef {
            bind: Binding::Const,
            scale: value,
        }
    }

    #[test]
    fn the_hash_is_stable_and_spreads() {
        assert_eq!(hash3(1, 2, 3), hash3(1, 2, 3));
        assert_ne!(hash3(1, 2, 3), hash3(1, 2, 4));
        assert_ne!(hash3(1, 2, 3), hash3(2, 1, 3));
        let mean: f32 = (0..1000).map(|i| unit_float(hash3(i, 7, 9))).sum::<f32>() / 1000.0;
        assert!((mean - 0.5).abs() < 0.05, "uniform mean {mean}");
        assert!((0..1000).all(|i| (0.0..1.0).contains(&unit_float(hash3(i, 0, 0)))));
    }

    #[test]
    fn a_disc_places_its_count_inside_its_radius() {
        let disc = PlacementDef::Disc {
            count: 5,
            radius: constant(2.0),
        };
        let bound = BoundValues::default();
        assert_eq!(instance_count(&disc, &bound, None), 5);
        for index in 0..5 {
            let placed = place(&disc, index, 42, &bound, None);
            assert!(placed.offset.length() <= 2.0 + 1e-5);
            assert!(placed.offset.y.abs() < 1e-6, "on the ground plane");
            assert_eq!(
                placed,
                place(&disc, index, 42, &bound, None),
                "deterministic"
            );
        }
        let many = PlacementDef::Disc {
            count: 500,
            radius: constant(1.0),
        };
        assert_eq!(
            instance_count(&many, &bound, None),
            MAX_INSTANCES_PER_PLACEMENT
        );
    }

    #[test]
    fn a_line_places_every_spacing_and_delays_by_travel() {
        let line = WorldLine {
            from: Vec3::ZERO,
            to: Vec3::new(9.0, 0.0, 0.0),
        };
        let placement = PlacementDef::Line {
            spacing: constant(1.5),
            travel: 3.0,
        };
        let bound = BoundValues::default();
        assert_eq!(instance_count(&placement, &bound, Some(line)), 7);
        let last = place(&placement, 6, 1, &bound, Some(line));
        assert!((last.offset.x - 9.0).abs() < 1e-5);
        assert!((last.delay - 3.0).abs() < 1e-5);
        assert_eq!(
            instance_count(&placement, &bound, None),
            1,
            "no line, one instance"
        );
        let long = WorldLine {
            from: Vec3::ZERO,
            to: Vec3::new(1000.0, 0.0, 0.0),
        };
        assert_eq!(
            instance_count(&placement, &bound, Some(long)),
            MAX_INSTANCES_PER_PLACEMENT
        );
    }
}
