//! Timing curves and colour ramps of the effect vocabulary, evaluated as pure functions.
//!
//! # Responsibility
//! Owns [`Curve`] (keys of `(time, value, easing)` over a layer's normalised life) and
//! [`Ramp`] (colour keys over luminance, baked once into a fixed row of
//! [`RAMP_TEXELS`] texels for the GPU ramp atlas).
//!
//! # How it fits
//! The catalog resolver validates and builds both from `effects.ron`; the runtime evaluates
//! curves for intensity, size and erosion each frame, and the renderer uploads the baked ramp
//! rows once per session.
//!
//! # Invariants
//! - Keys are strictly increasing in `0..=1` (checked at load); evaluation clamps outside them.
//! - Evaluation allocates nothing and depends only on its arguments.

use super::catalog::schema::{Easing, RampKey};

/// Texels in one baked ramp row.
pub const RAMP_TEXELS: usize = 256;

/// A timing curve over normalised time `0..=1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// `(time, value, easing to the next key)`, strictly increasing in time.
    keys: Vec<(f32, f32, Easing)>,
}

/// Why a list of keys is not a valid curve or ramp: the index of the first bad key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnorderedKey(pub usize);

/// Checks that `times` is non-empty, inside `0..=1` and strictly increasing.
///
/// # Errors
/// [`UnorderedKey`] with the first offending index (0 for an empty list).
pub fn check_key_times(times: impl Iterator<Item = f32>) -> Result<(), UnorderedKey> {
    let mut previous: Option<f32> = None;
    let mut count = 0;
    for (index, time) in times.enumerate() {
        count += 1;
        let ordered = previous.is_none_or(|before| time > before);
        if !(0.0..=1.0).contains(&time) || !ordered {
            return Err(UnorderedKey(index));
        }
        previous = Some(time);
    }
    if count == 0 {
        return Err(UnorderedKey(0));
    }
    Ok(())
}

/// The eased fraction `t` (0..=1) of the way from one key to the next.
#[must_use]
pub fn ease(easing: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::Step => 0.0,
        Easing::EaseInQuad => t * t,
        Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
        Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        Easing::EaseInOut => t * t * (3.0 - 2.0 * t),
    }
}

impl Curve {
    /// Builds a curve from validated keys.
    ///
    /// # Errors
    /// [`UnorderedKey`] when the keys are empty, outside `0..=1` or not strictly increasing.
    pub fn new(keys: Vec<(f32, f32, Easing)>) -> Result<Self, UnorderedKey> {
        check_key_times(keys.iter().map(|key| key.0))?;
        Ok(Self { keys })
    }

    /// The constant curve `value`.
    #[must_use]
    pub fn constant(value: f32) -> Self {
        Self {
            keys: vec![(0.0, value, Easing::Linear)],
        }
    }

    /// Value at normalised time `t`; before the first key the first value, after the last the
    /// last value.
    #[must_use]
    pub fn sample(&self, t: f32) -> f32 {
        let Some(&(first_time, first_value, _)) = self.keys.first() else {
            return 0.0;
        };
        if t <= first_time {
            return first_value;
        }
        for pair in self.keys.windows(2) {
            let (t0, v0, easing) = pair[0];
            let (t1, v1, _) = pair[1];
            if t <= t1 {
                let fraction = ease(easing, (t - t0) / (t1 - t0));
                return v0 + (v1 - v0) * fraction;
            }
        }
        self.keys.last().map_or(first_value, |key| key.1)
    }
}

/// A colour ramp baked into [`RAMP_TEXELS`] linear RGBA texels.
#[derive(Debug, Clone, PartialEq)]
pub struct Ramp {
    /// The baked row, luminance 0 first.
    texels: Vec<[f32; 4]>,
}

impl Ramp {
    /// Bakes a ramp from keys `(position, (r, g, b, a))`, interpolated linearly.
    ///
    /// # Errors
    /// [`UnorderedKey`] when the keys are empty, outside `0..=1` or not strictly increasing.
    pub fn bake(keys: &[RampKey]) -> Result<Self, UnorderedKey> {
        check_key_times(keys.iter().map(|key| key.0))?;
        let channel = |pick: fn(&(f32, f32, f32, f32)) -> f32| {
            Curve::new(
                keys.iter()
                    .map(|(time, colour)| (*time, pick(colour), Easing::Linear))
                    .collect(),
            )
        };
        let curves = [
            channel(|c| c.0)?,
            channel(|c| c.1)?,
            channel(|c| c.2)?,
            channel(|c| c.3)?,
        ];
        let texels = (0..=u8::MAX)
            .map(|texel| {
                let position = f32::from(texel) / f32::from(u8::MAX);
                curves.each_ref().map(|curve| curve.sample(position))
            })
            .collect();
        Ok(Self { texels })
    }

    /// The baked row.
    #[must_use]
    pub fn texels(&self) -> &[[f32; 4]] {
        &self.texels
    }

    /// The colour at luminance `position` (0..=1), from the nearest baked texel.
    #[must_use]
    pub fn at(&self, position: f32) -> [f32; 4] {
        let index = (position.clamp(0.0, 1.0) * f32::from(u8::MAX)).round();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "index is clamped to 0..=255 above"
        )]
        let index = index as usize;
        self.texels.get(index).copied().unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a failed test is a panic")]
mod tests {
    use super::*;

    fn punch() -> Curve {
        Curve::new(vec![
            (0.0, 0.0, Easing::EaseOutCubic),
            (0.1, 1.0, Easing::Linear),
            (1.0, 0.0, Easing::Linear),
        ])
        .expect("ordered keys")
    }

    #[test]
    fn a_curve_hits_its_keys_and_clamps_outside_them() {
        let curve = punch();
        assert!((curve.sample(0.0)).abs() < 1e-6);
        assert!((curve.sample(0.1) - 1.0).abs() < 1e-6);
        assert!((curve.sample(1.0)).abs() < 1e-6);
        assert!((curve.sample(-3.0)).abs() < 1e-6);
        assert!((curve.sample(7.0)).abs() < 1e-6);
        assert!((curve.sample(0.55) - 0.5).abs() < 1e-5, "linear fall");
    }

    #[test]
    fn easings_start_at_zero_and_end_at_one() {
        for easing in [
            Easing::Linear,
            Easing::EaseInQuad,
            Easing::EaseOutQuad,
            Easing::EaseOutCubic,
            Easing::EaseInOut,
        ] {
            assert!(ease(easing, 0.0).abs() < 1e-6, "{easing:?} at 0");
            assert!((ease(easing, 1.0) - 1.0).abs() < 1e-6, "{easing:?} at 1");
        }
        assert!(ease(Easing::EaseOutCubic, 0.5) > ease(Easing::Linear, 0.5));
        assert!(ease(Easing::EaseInQuad, 0.5) < ease(Easing::Linear, 0.5));
        assert!(ease(Easing::Step, 0.99).abs() < 1e-6, "a step holds");
    }

    #[test]
    fn unordered_empty_or_out_of_range_keys_are_refused() {
        assert_eq!(Curve::new(vec![]).expect_err("empty"), UnorderedKey(0));
        assert_eq!(
            Curve::new(vec![(0.5, 1.0, Easing::Linear), (0.5, 0.0, Easing::Linear)])
                .expect_err("repeated time"),
            UnorderedKey(1)
        );
        assert_eq!(
            Curve::new(vec![(0.0, 1.0, Easing::Linear), (1.5, 0.0, Easing::Linear)])
                .expect_err("beyond 1"),
            UnorderedKey(1)
        );
    }

    #[test]
    fn a_ramp_bakes_its_end_keys_into_its_end_texels() {
        let ramp = Ramp::bake(&[(0.0, (0.0, 0.0, 0.0, 0.0)), (1.0, (2.0, 1.0, 0.5, 1.0))])
            .expect("ordered keys");
        assert_eq!(ramp.texels().len(), RAMP_TEXELS);
        assert_eq!(ramp.texels()[0], [0.0; 4]);
        assert_eq!(ramp.texels()[RAMP_TEXELS - 1], [2.0, 1.0, 0.5, 1.0]);
        let middle = ramp.at(0.5);
        assert!(
            (middle[0] - 1.0).abs() < 0.01,
            "linear in between: {middle:?}"
        );
    }
}
