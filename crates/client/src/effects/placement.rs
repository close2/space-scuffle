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
use super::catalog::schema::{Binding, PlacementDef};
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
    /// Start of the line: its first search point.
    pub from: Vec3,
    /// End of the line.
    pub to: Vec3,
    /// Bit `i` set when the viewer sees search point `i` (counted from `from` in steps of the
    /// line's spacing); instances placed at a point the viewer does not see are not drawn
    /// (plan Amendment 2, R6). [`WorldLine::whole`] sets every bit.
    pub seen: u64,
}

impl WorldLine {
    /// A line whose every search point is seen: no fog, or a viewer who sees all of it.
    #[must_use]
    pub const fn whole(from: Vec3, to: Vec3) -> Self {
        Self {
            from,
            to,
            seen: u64::MAX,
        }
    }

    /// Whether the viewer sees search point `point`. Points from the 64th on share its bit.
    #[must_use]
    pub const fn sees_point(&self, point: u32) -> bool {
        let bit = if point < 63 { point } else { 63 };
        self.seen & (1 << bit) != 0
    }
}

/// Whether instance `index` of `placement` stands only at search points of `line` the viewer
/// sees: a point-placed instance needs the search point nearest to it, an instance between two
/// placed points needs every search point from the one to the other. Placed points are mapped
/// onto the line's search points through `bound`'s `LineSpacing`, so a placement spaced wider
/// than the line (a card every third point) asks for the points it actually stands on; without
/// a bound spacing the placement's points are the search points. Instances of other placements,
/// and of an effect without a line, are always seen.
#[must_use]
pub fn placed_seen(
    placement: &PlacementDef,
    index: u32,
    line: Option<WorldLine>,
    bound: &BoundValues,
) -> bool {
    let (
        PlacementDef::Line {
            spacing,
            between,
            per_point,
            ..
        },
        Some(line),
    ) = (placement, line)
    else {
        return true;
    };
    let point = index / (*per_point).max(1);
    let line_spacing = bound.get(Binding::LineSpacing);
    let ratio = if line_spacing > f32::EPSILON {
        resolve_value(spacing, bound) / line_spacing
    } else {
        1.0
    };
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "point indices below a few hundred, rounded from non-negative distances"
    )]
    let (first, last) = {
        let at = |placed: u32| (placed as f32 * ratio).round().max(0.0) as u32;
        (at(point), at(point + u32::from(*between)))
    };
    (first..=last).all(|search| line.sees_point(search))
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
        PlacementDef::Point | PlacementDef::LineEnd { .. } => 1,
        PlacementDef::Disc { count, .. } | PlacementDef::Circle { count, .. } => {
            (*count).min(MAX_INSTANCES_PER_PLACEMENT)
        }
        PlacementDef::Line {
            spacing,
            between,
            per_point,
            ..
        } => {
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
            let count = if *between { steps.max(1) } else { steps + 1 };
            count
                .saturating_mul((*per_point).max(1))
                .min(MAX_INSTANCES_PER_PLACEMENT)
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
        PlacementDef::LineEnd { back } => {
            let Some(line) = line else {
                return Placed {
                    offset: Vec3::ZERO,
                    delay: 0.0,
                };
            };
            let span = line.to - line.from;
            let length = span.length();
            let back = resolve_value(back, bound).clamp(0.0, length);
            Placed {
                offset: span.normalize_or_zero() * (length - back),
                delay: 0.0,
            }
        }
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
        PlacementDef::Circle {
            count,
            radius,
            spread,
        } => {
            let radius = resolve_value(radius, bound);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a placement makes at most MAX_INSTANCES_PER_PLACEMENT instances"
            )]
            let step = 1.0 / (*count).max(1) as f32;
            #[expect(
                clippy::cast_precision_loss,
                reason = "an index below MAX_INSTANCES_PER_PLACEMENT is exact in f32"
            )]
            let slot = index as f32;
            let start = unit_float(hash3(seed, 0, 0x0C12_C1E0));
            let jitter = (unit_float(hash3(seed, index, 0x0C12_C1E1)) - 0.5) * (2.0 / 3.0);
            let turn = start + (slot + jitter) * step;
            let distance =
                radius * (1.0 + spread * (2.0 * unit_float(hash3(seed, index, 0x0C12_C1E2)) - 1.0));
            let angle = std::f32::consts::TAU * turn;
            Placed {
                offset: Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance),
                delay: 0.0,
            }
        }
        PlacementDef::Line {
            spacing,
            travel,
            between,
            per_point,
            scatter,
            delay_jitter,
        } => {
            let Some(line) = line else {
                return Placed {
                    offset: Vec3::ZERO,
                    delay: 0.0,
                };
            };
            let direction = (line.to - line.from).normalize_or_zero();
            let across = Vec3::new(-direction.z, 0.0, direction.x);
            let point = index / (*per_point).max(1);
            #[expect(clippy::cast_precision_loss, reason = "point is below 32")]
            let steps = point as f32 + if *between { 0.5 } else { 0.0 };
            let distance = resolve_value(spacing, bound) * steps;
            let delay = if *travel > f32::EPSILON {
                distance / travel
            } else {
                0.0
            };
            let signed = |salt: u32| 2.0 * unit_float(hash3(seed, index, salt)) - 1.0;
            let scattered =
                (direction * signed(0x5CA7_0001) + across * signed(0x5CA7_0002)) * *scatter;
            Placed {
                offset: direction * distance + scattered,
                delay: delay + *delay_jitter * unit_float(hash3(seed, index, 0x0DE1_A700)),
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
    fn a_circle_places_its_count_evenly_round_its_radius() {
        let circle = PlacementDef::Circle {
            count: 8,
            radius: constant(2.0),
            spread: 0.1,
        };
        let bound = BoundValues::default();
        assert_eq!(instance_count(&circle, &bound, None), 8);
        let mut angles: Vec<f32> = (0..8)
            .map(|index| {
                let placed = place(&circle, index, 42, &bound, None);
                let distance = placed.offset.length();
                assert!(
                    (1.8 - 1e-4..=2.2 + 1e-4).contains(&distance),
                    "on the circle within its spread: {distance}"
                );
                assert!(placed.offset.y.abs() < 1e-6, "on the ground plane");
                assert_eq!(
                    placed,
                    place(&circle, index, 42, &bound, None),
                    "deterministic"
                );
                placed
                    .offset
                    .z
                    .atan2(placed.offset.x)
                    .rem_euclid(std::f32::consts::TAU)
            })
            .collect();
        angles.sort_by(f32::total_cmp);
        // Evenly round: no gap between neighbours wider than a spacing and two thirds of one
        // (each turned by at most a third either way).
        let spacing = std::f32::consts::TAU / 8.0;
        for (index, angle) in angles.iter().enumerate() {
            let next = angles
                .get(index + 1)
                .copied()
                .unwrap_or(angles[0] + std::f32::consts::TAU);
            assert!(next - angle <= spacing * (5.0 / 3.0) + 1e-4, "{angles:?}");
        }
        // Another effect starts the circle elsewhere.
        assert_ne!(
            place(&circle, 0, 42, &bound, None),
            place(&circle, 0, 43, &bound, None)
        );
    }

    #[test]
    fn a_line_places_every_spacing_and_delays_by_travel() {
        let line = WorldLine::whole(Vec3::ZERO, Vec3::new(9.0, 0.0, 0.0));
        let placement = PlacementDef::Line {
            spacing: constant(1.5),
            travel: 3.0,
            between: false,
            per_point: 1,
            scatter: 0.0,
            delay_jitter: 0.0,
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
        let long = WorldLine::whole(Vec3::ZERO, Vec3::new(1000.0, 0.0, 0.0));
        assert_eq!(
            instance_count(&placement, &bound, Some(long)),
            MAX_INSTANCES_PER_PLACEMENT
        );
    }

    #[test]
    fn segments_between_the_points_end_at_the_line_ends() {
        let line = WorldLine::whole(Vec3::ZERO, Vec3::new(8.0, 0.0, 0.0));
        let placement = PlacementDef::Line {
            spacing: constant(1.0),
            travel: 4.0,
            between: true,
            per_point: 1,
            scatter: 0.0,
            delay_jitter: 0.0,
        };
        let bound = BoundValues::default();
        assert_eq!(instance_count(&placement, &bound, Some(line)), 8);
        let first = place(&placement, 0, 1, &bound, Some(line));
        let last = place(&placement, 7, 1, &bound, Some(line));
        assert!((first.offset.x - 0.5).abs() < 1e-5);
        assert!((last.offset.x - 7.5).abs() < 1e-5);
        assert!(
            (last.delay - 7.5 / 4.0).abs() < 1e-5,
            "delayed by its distance"
        );
    }

    #[test]
    fn a_clump_per_point_scatters_and_jitters_within_its_bounds() {
        let line = WorldLine::whole(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0));
        let placement = PlacementDef::Line {
            spacing: constant(1.0),
            travel: 10.0,
            between: false,
            per_point: 3,
            scatter: 0.25,
            delay_jitter: 0.05,
        };
        let bound = BoundValues::default();
        assert_eq!(
            instance_count(&placement, &bound, Some(line)),
            15,
            "five points, three each"
        );
        let mut spread = 0.0_f32;
        for index in 0..15 {
            let placed = place(&placement, index, 7, &bound, Some(line));
            #[expect(clippy::cast_precision_loss, reason = "a small index")]
            let point = (index / 3) as f32;
            let along = placed.offset.x - point;
            assert!(along.abs() <= 0.25 + 1e-5 && placed.offset.z.abs() <= 0.25 + 1e-5);
            let base_delay = point / 10.0;
            assert!(placed.delay >= base_delay - 1e-6 && placed.delay <= base_delay + 0.05 + 1e-6);
            spread = spread.max(placed.offset.z.abs());
        }
        assert!(spread > 0.05, "the clump is scattered across the line");
    }

    #[test]
    fn only_instances_at_seen_points_are_seen() {
        let mut line = WorldLine::whole(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0));
        line.seen = 0b0_0110;
        let clumps = PlacementDef::Line {
            spacing: constant(1.0),
            travel: 10.0,
            between: false,
            per_point: 2,
            scatter: 0.0,
            delay_jitter: 0.0,
        };
        let seen: Vec<bool> = (0..10)
            .map(|index| placed_seen(&clumps, index, Some(line), &BoundValues::default()))
            .collect();
        assert_eq!(
            seen,
            [false, false, true, true, true, true, false, false, false, false],
            "points 1 and 2, two instances each"
        );
        let segments = PlacementDef::Line {
            spacing: constant(1.0),
            travel: 10.0,
            between: true,
            per_point: 1,
            scatter: 0.0,
            delay_jitter: 0.0,
        };
        let seen: Vec<bool> = (0..4)
            .map(|index| placed_seen(&segments, index, Some(line), &BoundValues::default()))
            .collect();
        assert_eq!(
            seen,
            [false, true, false, false],
            "only the segment from 1 to 2"
        );
        assert!(
            placed_seen(&PlacementDef::Point, 0, Some(line), &BoundValues::default()),
            "not a line placement"
        );
        assert!(
            placed_seen(&clumps, 0, None, &BoundValues::default()),
            "no line"
        );
        // A card every third search point stands on points 0, 3, 6, ...: the second card
        // (point 3) is not seen while only points 1 and 2 are.
        let wide = PlacementDef::Line {
            spacing: ValueDef {
                bind: Binding::LineSpacing,
                scale: 3.0,
            },
            travel: 0.0,
            between: false,
            per_point: 1,
            scatter: 0.0,
            delay_jitter: 0.0,
        };
        let spaced = BoundValues::default().with(Binding::LineSpacing, 0.5);
        let mut long = WorldLine::whole(Vec3::ZERO, Vec3::new(4.5, 0.0, 0.0));
        long.seen = 0b1000;
        let seen: Vec<bool> = (0..4)
            .map(|index| placed_seen(&wide, index, Some(long), &spaced))
            .collect();
        assert_eq!(
            seen,
            [false, true, false, false],
            "only the card on point 3"
        );
        let mut far = WorldLine::whole(Vec3::ZERO, Vec3::X);
        far.seen = 1 << 63;
        assert!(far.sees_point(63) && far.sees_point(200) && !far.sees_point(62));
    }

    /// WP10 review 3, A3-1 (b): a grip set back from the line's far end by a fixed distance,
    /// whatever the line's length, and never past its start.
    #[test]
    fn a_line_end_placement_stands_a_fixed_distance_back_from_the_far_end() {
        let placement = PlacementDef::LineEnd {
            back: constant(0.12),
        };
        let bound = BoundValues::default();
        for length in [1.0_f32, 3.6, 9.4] {
            let line = WorldLine::whole(Vec3::ZERO, Vec3::new(length, 0.0, 0.0));
            assert_eq!(instance_count(&placement, &bound, Some(line)), 1);
            let placed = place(&placement, 0, 7, &bound, Some(line));
            assert!(
                (placed.offset.x - (length - 0.12)).abs() < 1e-5,
                "{length}: {placed:?}"
            );
            assert!(placed_seen(&placement, 0, Some(line), &bound));
        }
        let short = WorldLine::whole(Vec3::ZERO, Vec3::new(0.05, 0.0, 0.0));
        assert_eq!(
            place(&placement, 0, 7, &bound, Some(short)).offset,
            Vec3::ZERO
        );
        assert_eq!(place(&placement, 0, 7, &bound, None).offset, Vec3::ZERO);
    }
}
