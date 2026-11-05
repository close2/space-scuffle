//! Host-independent transcendental functions for the smog presentation.
//!
//! # Responsibility
//! Every exponential, sine, cosine, arctangent and power that reaches smog output is computed
//! here, through the pure-Rust [`libm`] crate, instead of through `f32::exp` and friends, which
//! call the host's C math library. The last bit of `expf`, `sinf`, `cosf`, `atan2f` and `powf`
//! differs between glibc versions, musl, macOS and Windows; the `libm` crate's results do not.
//!
//! # How it fits
//! The fluid solver (`fluid.rs`), the bouncing-bubble kinematics (`kinematics.rs`) and the smog
//! systems (`mod.rs`) call these functions on every path the smog goldens of
//! `crates/client/tests/smog_golden_tests.rs` hash, so those goldens pin the code and not the
//! host (owner question Q0014, option b; plan `docs/plans/2026-11-04-libm-smog-math.md`). The rest
//! of the client keeps the host's faster library: this module is deliberately smog-local (plan
//! D-2), and widening it to the whole client would be `bevy_math`'s `libm` feature instead.
//! [`srgb_to_linear`] and [`quat_from_rotation_y`] restate the Bevy and glam formulas the smog
//! code used before, evaluated through this module's functions.
//!
//! # Invariants
//! - Every function returns the same bits on every target the client builds for (`x86_64`,
//!   `aarch64`, `wasm32`): the `libm` crate computes them with ordinary IEEE `f32` and `f64`
//!   operations, Rust never contracts them into fused multiply-adds, and the crate replaces only
//!   exactly rounded operations (`sqrt`, `floor`, ...) with instructions.
//! - The results may change between `libm` crate versions; the golden file's
//!   `smog_libm_crate_canary` pins them, so a version bump that changes them fails loudly.
//! - Smog production code outside this file never calls a `std`, Bevy or glam transcendental
//!   function (workspace check C23).
//!
//! The module is public only so the informational micro-benchmark
//! `crates/client/benches/smog_math.rs` can time it against the host's functions.

use bevy::color::LinearRgba;
use bevy::math::{Quat, Vec3};

/// `e^x` for any `f32` `x`; same bits on every host (`libm::expf`).
#[inline]
#[must_use]
pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}

/// Sine of `x` radians; same bits on every host (`libm::sinf`).
#[inline]
#[must_use]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}

/// Cosine of `x` radians; same bits on every host (`libm::cosf`).
#[inline]
#[must_use]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}

/// `(sin x, cos x)` of `x` radians; same bits on every host (`libm::sincosf`).
#[inline]
#[must_use]
pub fn sin_cos(x: f32) -> (f32, f32) {
    libm::sincosf(x)
}

/// Angle in radians, in `[-π, π]`, of the point `(x, y)` from the +x axis towards +y; same bits
/// on every host (`libm::atan2f`). Argument order is that of `f32::atan2` called as `y.atan2(x)`.
#[inline]
#[must_use]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

/// `x` raised to the real power `y`; same bits on every host (`libm::powf`).
#[inline]
#[must_use]
pub fn powf(x: f32, y: f32) -> f32 {
    libm::powf(x, y)
}

/// One sRGB-encoded channel in `[0, 1]` to linear light, with Bevy 0.15's formula
/// (`bevy_color-0.15.4/src/srgba.rs`, `Srgba::gamma_function`) evaluated through [`powf`].
#[inline]
#[must_use]
fn srgb_channel_to_linear(value: f32) -> f32 {
    if value <= 0.0 {
        return value;
    }
    if value <= 0.04045 {
        value / 12.92
    } else {
        powf((value + 0.055) / 1.055, 2.4)
    }
}

/// An sRGB-encoded colour (each channel in `[0, 1]`) as linear RGBA with alpha 1, as
/// `Color::srgb(c.x, c.y, c.z).to_linear()` computes it in Bevy 0.15, but through [`powf`].
#[inline]
#[must_use]
pub fn srgb_to_linear(c: Vec3) -> LinearRgba {
    LinearRgba::new(
        srgb_channel_to_linear(c.x),
        srgb_channel_to_linear(c.y),
        srgb_channel_to_linear(c.z),
        1.0,
    )
}

/// Rotation by `angle` radians about +Y (counter-clockwise looking down from +Y), built as glam
/// 0.29's `Quat::from_rotation_y` (`glam-0.29.3/src/f32/sse2/quat.rs`: `(s, c) =
/// sin_cos(angle * 0.5)`, quaternion `(0, s, 0, c)`) but through [`sin_cos`].
#[inline]
#[must_use]
pub fn quat_from_rotation_y(angle: f32) -> Quat {
    let (s, c) = sin_cos(angle * 0.5);
    Quat::from_xyzw(0.0, s, 0.0, c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::color::Color;

    /// Number of arguments in each agreement sweep.
    const SWEEP: usize = 10_000;

    /// Largest distance, in units in the last place, allowed between this module and the host's
    /// function it replaces.
    const MAX_ULP: u32 = 2;

    /// Distance in units in the last place between two finite `f32` of any sign.
    fn ulp_distance(a: f32, b: f32) -> u32 {
        // Map the sign-magnitude bit patterns onto one monotonic integer line.
        fn ordered(v: f32) -> i64 {
            let bits = i64::from(v.to_bits());
            if bits & 0x8000_0000 == 0 {
                bits
            } else {
                0x8000_0000 - bits
            }
        }
        u32::try_from((ordered(a) - ordered(b)).unsigned_abs()).unwrap_or(u32::MAX)
    }

    /// `SWEEP` evenly spaced arguments from `lo` to `hi` inclusive.
    #[expect(
        clippy::cast_precision_loss,
        reason = "sweep indices stay far below 2^24"
    )]
    fn sweep(lo: f32, hi: f32) -> impl Iterator<Item = f32> {
        (0..SWEEP).map(move |i| lo + (hi - lo) * (i as f32 / (SWEEP - 1) as f32))
    }

    fn assert_close(name: &str, x: f32, ours: f32, host: f32) {
        let d = ulp_distance(ours, host);
        assert!(
            d <= MAX_ULP,
            "{name}({x}) = {ours} but the host gives {host}: {d} ulp apart"
        );
    }

    #[test]
    fn functions_are_the_libm_crate_bit_for_bit() {
        for x in sweep(-20.0, 20.0) {
            assert_eq!(exp(x).to_bits(), libm::expf(x).to_bits());
            assert_eq!(sin(x).to_bits(), libm::sinf(x).to_bits());
            assert_eq!(cos(x).to_bits(), libm::cosf(x).to_bits());
            let (s, c) = sin_cos(x);
            let (ls, lc) = libm::sincosf(x);
            assert_eq!((s.to_bits(), c.to_bits()), (ls.to_bits(), lc.to_bits()));
            assert_eq!(atan2(x, 1.5).to_bits(), libm::atan2f(x, 1.5).to_bits());
            let base = x.abs() * 0.05;
            assert_eq!(powf(base, 1.3).to_bits(), libm::powf(base, 1.3).to_bits());
        }
    }

    #[test]
    fn exp_agrees_with_host_over_the_decay_range() {
        // Decay exponents are `-rate * dt`: rates up to tens per second, dt up to 0.07 s.
        for x in sweep(-10.0, 0.0) {
            assert_close("exp", x, exp(x), x.exp());
        }
    }

    #[test]
    fn trig_agrees_with_host_over_the_call_site_ranges() {
        // Noise phases reach about 6 * TAU; crowd phases reach i * golden angle for i < 600.
        for x in sweep(-40.0, 40.0).chain(sweep(-1500.0, 1500.0)) {
            assert_close("sin", x, sin(x), x.sin());
            assert_close("cos", x, cos(x), x.cos());
            let (s, c) = sin_cos(x);
            assert_close("sin_cos.0", x, s, x.sin());
            assert_close("sin_cos.1", x, c, x.cos());
        }
    }

    #[test]
    fn atan2_agrees_with_host_around_the_circle() {
        for a in sweep(-std::f32::consts::PI, std::f32::consts::PI) {
            let (y, x) = (3.0 * a.sin(), 3.0 * a.cos());
            assert_close("atan2", a, atan2(y, x), y.atan2(x));
        }
        assert_eq!(atan2(0.0, 1.0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(atan2(0.0, -1.0), std::f32::consts::PI);
    }

    #[test]
    fn powf_agrees_with_host_over_the_noise_range() {
        for base in sweep(0.0, 1.0) {
            for exponent in [1.25, 1.30, 1.35, 1.40, 2.4] {
                assert_close("powf", base, powf(base, exponent), base.powf(exponent));
            }
        }
    }

    #[test]
    fn srgb_to_linear_agrees_with_bevy() {
        for v in sweep(0.0, 1.0) {
            let ours = srgb_to_linear(Vec3::new(v, 1.0 - v, v * 0.5));
            let bevy = Color::srgb(v, 1.0 - v, v * 0.5).to_linear();
            assert_close("srgb_to_linear.r", v, ours.red, bevy.red);
            assert_close("srgb_to_linear.g", v, ours.green, bevy.green);
            assert_close("srgb_to_linear.b", v, ours.blue, bevy.blue);
            assert_eq!(ours.alpha.to_bits(), bevy.alpha.to_bits());
        }
    }

    #[test]
    fn srgb_to_linear_is_exact_at_black_and_white() {
        assert_eq!(
            srgb_to_linear(Vec3::ZERO),
            LinearRgba::new(0.0, 0.0, 0.0, 1.0)
        );
        assert_eq!(
            srgb_to_linear(Vec3::ONE),
            LinearRgba::new(1.0, 1.0, 1.0, 1.0)
        );
    }

    #[test]
    fn quat_from_rotation_y_agrees_with_glam() {
        for angle in sweep(-std::f32::consts::TAU, std::f32::consts::TAU) {
            let ours = quat_from_rotation_y(angle);
            let glam = Quat::from_rotation_y(angle);
            for (lane, (o, g)) in ours.to_array().into_iter().zip(glam.to_array()).enumerate() {
                assert_close(&format!("quat_from_rotation_y lane {lane}"), angle, o, g);
            }
        }
        assert_eq!(quat_from_rotation_y(0.0), Quat::IDENTITY);
    }
}
