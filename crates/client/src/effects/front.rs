//! The guaranteed crests of a torn pressure front (plan `2026-11-09-effect-visualization`,
//! Amendment 8): where a torn front's leading edge stays on its radius whatever its noise.
//!
//! # Responsibility
//! Owns the placement of a torn front's [`FRONT_CRESTS`] crests: one per `360 / FRONT_CRESTS`
//! degrees, all turned by a per-instance rotation and each moved by a per-crest jitter of up to
//! [`CREST_JITTER_DEG`], both drawn from the instance's [`SpriteInstance::front_seed`] by
//! [`crest_hash`]. Between the crests the front's noise bites its edge back freely; on a crest
//! (within [`CREST_HALF_WIDTH_DEG`] of its centre) the edge is on the radius.
//!
//! # How it fits
//! `assets/shaders/effects/sprite.wgsl` (`front_texel`) computes the same crests with the same
//! integer hash, so this module is the shader's placement written in Rust: its tests prove, for
//! many seeds, the property the footprint test asks of one capture (every 45-degree sector of
//! the front has dust on the reach) and that two hits do not share an outline.
//!
//! # Invariants
//! - The largest gap between neighbouring crest centres is below 45 degrees for every seed:
//!   `360 / FRONT_CRESTS + 2 * CREST_JITTER_DEG` = 44 degrees.
//! - The hash is integer arithmetic only, so the shader and this module agree bit for bit.
//!
//! [`SpriteInstance::front_seed`]: super::instances::SpriteInstance::front_seed

/// Crests of a torn front around its circle.
pub const FRONT_CRESTS: u32 = 12;

/// Largest jitter of a crest from its even place, degrees.
pub const CREST_JITTER_DEG: f32 = 7.0;

/// Half the width of a crest, degrees: within it the leading edge is on the radius. Half the
/// footprint test's 5-degree ray spacing, so a ray always falls on every crest.
pub const CREST_HALF_WIDTH_DEG: f32 = 2.5;

/// A 32-bit integer hash (PCG output permutation), the same in the effect shader.
#[must_use]
pub const fn crest_hash(value: u32) -> u32 {
    let state = value.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}

/// `value`'s top 24 bits as a fraction in [0, 1), as the shader converts it.
#[must_use]
pub fn unit(value: u32) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "24 bits are exact in f32")]
    let fraction = (value >> 8) as f32 / 16_777_216.0;
    fraction
}

/// The centres of the crests of the front drawn with `seed`, in turns (0 to 1, not sorted
/// when a jitter crosses 0).
#[must_use]
pub fn crest_centres(seed: u32) -> [f32; FRONT_CRESTS as usize] {
    let rotation = unit(crest_hash(seed ^ 0x9E37_79B9));
    #[expect(
        clippy::cast_precision_loss,
        reason = "a dozen crests are exact in f32"
    )]
    let crests = FRONT_CRESTS as f32;
    std::array::from_fn(|index| {
        let index = u32::try_from(index).unwrap_or(0);
        let jitter =
            (2.0 * unit(crest_hash(
                seed.wrapping_add(index.wrapping_mul(0x85EB_CA6B)),
            )) - 1.0)
                * CREST_JITTER_DEG
                / 360.0;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a dozen crests are exact in f32"
        )]
        let place = (index as f32 + rotation) / crests;
        (place + jitter).rem_euclid(1.0)
    })
}

/// The circular distance between two angles in turns, in turns (0 to 0.5).
#[must_use]
pub fn turn_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::placement::hash3;

    /// The ring card seeds of 64 consecutive hits, drawn as the runtime draws them (the
    /// instance seed from the spawn sequence, then the layer's placed-instance hash).
    fn seeds() -> impl Iterator<Item = u32> {
        (0..64_u32).map(|hit| hash3(hash3(hit, 0x5EED_0F1E, 0x00C1_EA7E), 12, 0))
    }

    #[test]
    fn the_largest_gap_between_crests_is_below_a_sector_for_every_seed() {
        for seed in seeds() {
            let mut centres = crest_centres(seed).to_vec();
            centres.sort_by(f32::total_cmp);
            let mut largest = 1.0 - centres[centres.len() - 1] + centres[0];
            for pair in centres.windows(2) {
                largest = largest.max(pair[1] - pair[0]);
            }
            assert!(
                largest * 360.0 < 45.0,
                "seed {seed:#x}: a gap of {:.1} degrees",
                largest * 360.0
            );
        }
    }

    /// The footprint test's sectors (72 rays, 8 sectors of 9) each hold a ray on a crest.
    #[test]
    fn every_footprint_sector_has_a_ray_on_a_crest_for_every_seed() {
        for seed in seeds() {
            let centres = crest_centres(seed);
            for sector in 0..8_u16 {
                let on_crest = (0..9_u16).any(|ray| {
                    let angle = f32::from(sector * 9 + ray) / 72.0;
                    centres.iter().any(|centre| {
                        turn_distance(angle, *centre) * 360.0 <= CREST_HALF_WIDTH_DEG + 1e-3
                    })
                });
                assert!(
                    on_crest,
                    "seed {seed:#x}: sector {sector} has no ray on a crest"
                );
            }
        }
    }

    /// Two consecutive hits do not share an outline: the mean distance from each crest of one
    /// to the nearest crest of the next is at least 1 degree for every pair and 5 degrees on
    /// average (an even rotation alone gives 7.5).
    #[test]
    fn two_hits_place_their_crests_differently() {
        let all: Vec<u32> = seeds().collect();
        let mut total = 0.0;
        for pair in all.windows(2) {
            let (a, b) = (crest_centres(pair[0]), crest_centres(pair[1]));
            let mean = a
                .iter()
                .map(|centre| {
                    b.iter()
                        .map(|other| turn_distance(*centre, *other))
                        .fold(f32::MAX, f32::min)
                })
                .sum::<f32>()
                * 360.0
                / 12.0;
            assert!(
                mean >= 1.0,
                "seeds {:#x}, {:#x}: {mean:.2} degrees",
                pair[0],
                pair[1]
            );
            total += mean;
        }
        let average = total / 63.0;
        assert!(average >= 5.0, "average {average:.2} degrees");
    }

    #[test]
    fn the_hash_spreads_and_is_stable() {
        assert_ne!(crest_hash(0), crest_hash(1));
        assert_eq!(crest_hash(12_345), crest_hash(12_345));
        assert!((0..1000)
            .map(crest_hash)
            .map(unit)
            .all(|u| (0.0..1.0).contains(&u)));
    }
}
