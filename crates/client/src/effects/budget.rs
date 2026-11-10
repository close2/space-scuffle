//! Budget and level-of-detail rules of the effect system, as pure functions (plan §5.6).
//!
//! # Responsibility
//! Decides, from numbers only, what is drawn when pools fill or effects are small or crowded:
//! - [`projected_px`]: the on-screen size of a world-space size.
//! - [`lod_skips`]: a layer below `min_screen_px` is skipped.
//! - [`particle_scale`]: burst counts scale with projected size between 0.25 and 1.0.
//! - [`crowd_factor`]: above `crowd_threshold` live on-screen `Major` effects, detail bursts
//!   halve.
//! - [`burst_count`]: the particles one burst spawns.
//! - [`layer_admitted`]: `Critical` layers always draw; a `Detail` layer draws only into a free
//!   slot.
//! - [`retirement_candidate`]: which effect's detail is retired when a pool overflows: the
//!   oldest `Minor` or `Ambient` effect whose detail still draws; `Major` never.
//! - [`retire_fade`]: retired detail erodes out over [`RETIRE_SECS`], never pops.
//!
//! # How it fits
//! The runtime calls these while it builds each frame's instances; unit tests pin every rule.
//!
//! # Invariants
//! - No function here allocates, reads a clock or holds state.

use super::catalog::schema::{BurstCount, EffectClass, Priority};

/// Seconds over which retired detail erodes out.
pub const RETIRE_SECS: f32 = 0.1;
/// Smallest share of a burst kept by the size rule.
pub const MIN_PARTICLE_SCALE: f32 = 0.25;

/// Height in pixels of a world-space length `size` seen at `distance` world units by a
/// perspective camera of vertical field of view `fov_y` (radians) on a viewport
/// `viewport_px` pixels high.
#[must_use]
pub fn projected_px(size: f32, distance: f32, fov_y: f32, viewport_px: f32) -> f32 {
    let distance = distance.max(1e-3);
    let visible_height = 2.0 * distance * (fov_y * 0.5).tan();
    if visible_height <= f32::EPSILON {
        return f32::INFINITY;
    }
    size / visible_height * viewport_px
}

/// Whether a layer of projected size `px` is too small to draw.
#[must_use]
pub fn lod_skips(px: f32, min_screen_px: f32) -> bool {
    px < min_screen_px
}

/// The share of a burst spawned at projected size `px`, from [`MIN_PARTICLE_SCALE`] to 1,
/// reaching 1 at `full_px`.
#[must_use]
pub fn particle_scale(px: f32, full_px: f32) -> f32 {
    (px / full_px.max(1.0)).clamp(MIN_PARTICLE_SCALE, 1.0)
}

/// The factor on detail bursts with `major_on_screen` live `Major` effects on screen.
#[must_use]
pub const fn crowd_factor(major_on_screen: u32, crowd_threshold: u32) -> f32 {
    if major_on_screen > crowd_threshold {
        0.5
    } else {
        1.0
    }
}

/// Particles of one burst of `count` for a layer of radius `radius`, scaled by `scale`
/// (size and crowd rules multiplied): at least 1 when the scaled count is positive.
#[must_use]
pub fn burst_count(count: &BurstCount, radius: f32, scale: f32) -> u32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "particle counts are far below 2^24"
    )]
    let (base, max) = (count.base as f32, count.max as f32);
    let wanted = base + count.per_unit_radius * radius.max(0.0);
    let capped = wanted.min(max) * scale.clamp(0.0, 1.0);
    if capped <= 0.0 {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "capped is positive and at most count.max"
    )]
    let rounded = capped.round().max(1.0) as u32;
    rounded.min(count.max)
}

/// Whether a layer of `priority` draws when its pool has `free` slots left.
///
/// A `Critical` layer always draws: its pools are sized to the measured peak (§5.6), so a
/// full pool there is a sizing defect the runtime counts, not a reason to hide gameplay.
#[must_use]
pub const fn layer_admitted(priority: Priority, free: u32) -> bool {
    match priority {
        Priority::Critical => true,
        Priority::Detail => free > 0,
    }
}

/// What the retirement rule knows of one live effect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetireCandidate {
    /// Budget class.
    pub class: EffectClass,
    /// Birth time in seconds.
    pub birth: f64,
    /// Whether its detail already retires.
    pub detail_retired: bool,
    /// Whether it has detail layers at all.
    pub has_detail: bool,
}

/// Index in `candidates` of the effect whose detail retires next: the oldest `Minor` or
/// `Ambient` effect with detail still drawing. `None` when there is none.
#[must_use]
pub fn retirement_candidate(
    candidates: impl Iterator<Item = (usize, RetireCandidate)>,
) -> Option<usize> {
    candidates
        .filter(|(_, candidate)| {
            candidate.class != EffectClass::Major
                && candidate.has_detail
                && !candidate.detail_retired
        })
        .min_by(|(_, a), (_, b)| a.birth.total_cmp(&b.birth))
        .map(|(index, _)| index)
}

/// Opacity of retired detail `retired_for` seconds after retirement: 1 down to 0 over
/// [`RETIRE_SECS`].
#[must_use]
pub fn retire_fade(retired_for: f32) -> f32 {
    (1.0 - retired_for / RETIRE_SECS).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_size_falls_with_distance_and_rises_with_viewport() {
        let fov = std::f32::consts::FRAC_PI_4;
        let near = projected_px(1.0, 10.0, fov, 720.0);
        let far = projected_px(1.0, 20.0, fov, 720.0);
        assert!((near - 2.0 * far).abs() < 1e-3);
        let expected = 720.0 / (20.0 * (fov / 2.0).tan());
        assert!((near - expected).abs() < 1e-3);
        assert!(projected_px(1.0, 10.0, fov, 1440.0) > near);
    }

    #[test]
    fn layers_below_the_minimum_size_are_skipped() {
        assert!(lod_skips(5.9, 6.0));
        assert!(!lod_skips(6.0, 6.0));
        assert!(!lod_skips(0.0, 0.0));
    }

    #[test]
    fn particle_scale_runs_from_a_quarter_to_one() {
        assert!((particle_scale(1.0, 96.0) - MIN_PARTICLE_SCALE).abs() < 1e-6);
        assert!((particle_scale(48.0, 96.0) - 0.5).abs() < 1e-6);
        assert!((particle_scale(500.0, 96.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_crowd_halves_detail_only_above_its_threshold() {
        assert!((crowd_factor(24, 24) - 1.0).abs() < 1e-6);
        assert!((crowd_factor(25, 24) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_burst_grows_with_radius_up_to_its_maximum_and_scales() {
        let count = BurstCount {
            base: 10,
            per_unit_radius: 8.0,
            max: 40,
        };
        assert_eq!(burst_count(&count, 0.0, 1.0), 10);
        assert_eq!(burst_count(&count, 1.0, 1.0), 18);
        assert_eq!(burst_count(&count, 10.0, 1.0), 40);
        assert_eq!(burst_count(&count, 10.0, 0.5), 20);
        assert_eq!(
            burst_count(&count, 0.0, 0.01),
            1,
            "a scaled burst keeps one particle"
        );
        assert_eq!(burst_count(&count, 0.0, 0.0), 0);
    }

    #[test]
    fn critical_layers_are_never_dropped_and_detail_needs_a_slot() {
        assert!(layer_admitted(Priority::Critical, 0));
        assert!(layer_admitted(Priority::Detail, 1));
        assert!(!layer_admitted(Priority::Detail, 0));
    }

    #[test]
    fn the_oldest_minor_or_ambient_detail_retires_first_and_major_never() {
        let candidate = |class, birth, detail_retired| RetireCandidate {
            class,
            birth,
            detail_retired,
            has_detail: true,
        };
        let live = [
            candidate(EffectClass::Major, 0.0, false),
            candidate(EffectClass::Minor, 2.0, false),
            candidate(EffectClass::Ambient, 1.0, false),
            candidate(EffectClass::Minor, 0.5, true),
        ];
        assert_eq!(
            retirement_candidate(live.iter().copied().enumerate()),
            Some(2),
            "the oldest not yet retired, skipping Major"
        );
        let majors = [candidate(EffectClass::Major, 0.0, false)];
        assert_eq!(
            retirement_candidate(majors.iter().copied().enumerate()),
            None
        );
        let no_detail = [RetireCandidate {
            has_detail: false,
            ..candidate(EffectClass::Minor, 0.0, false)
        }];
        assert_eq!(
            retirement_candidate(no_detail.iter().copied().enumerate()),
            None
        );
    }

    #[test]
    fn retired_detail_erodes_over_a_tenth_of_a_second() {
        assert!((retire_fade(0.0) - 1.0).abs() < 1e-6);
        assert!((retire_fade(0.05) - 0.5).abs() < 1e-5);
        assert!(retire_fade(0.1).abs() < 1e-6);
        assert!(retire_fade(3.0).abs() < 1e-6);
    }
}
