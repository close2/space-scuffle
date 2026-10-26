//! Deterministic fixed-point math: scalars (`SimScalar`), vectors (`SimVec2`), and angles (`SimAngle`).
//!
//! Space Scuffle is designed as a lockstep deterministic simulation. To eliminate IEEE 754
//! floating-point divergence across compiler targets, operating systems, and architectures,
//! authoritative simulation state uses 64-bit signed fixed-point numbers with 32 integer
//! and 32 fractional bits (Q32.32).
//!
//! Angular orientation is represented as discrete Binary Radians (`u16`), where modular 360-degree
//! circle wrapping is handled naturally by hardware integer overflow with zero branch misprediction.

use crate::errors::MathError;
use crate::sin_table::SIN_TABLE;
use fixed::types::I32F32;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Deterministic 64-bit fixed-point scalar with 32 integer and 32 fractional bits (Q32.32).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct SimScalar(pub I32F32);

impl Serialize for SimScalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(self.0.to_num::<f64>())
    }
}

impl<'de> Deserialize<'de> for SimScalar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SimScalarVisitor;

        impl<'de> serde::de::Visitor<'de> for SimScalarVisitor {
            type Value = SimScalar;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a float, integer, or fixed-point scalar")
            }

            fn visit_f64<E>(self, v: f64) -> Result<SimScalar, E>
            where
                E: serde::de::Error,
            {
                Ok(SimScalar::from_num(v))
            }

            fn visit_f32<E>(self, v: f32) -> Result<SimScalar, E>
            where
                E: serde::de::Error,
            {
                Ok(SimScalar::from_num(v))
            }

            fn visit_i64<E>(self, v: i64) -> Result<SimScalar, E>
            where
                E: serde::de::Error,
            {
                Ok(SimScalar::from_num(v))
            }

            fn visit_u64<E>(self, v: u64) -> Result<SimScalar, E>
            where
                E: serde::de::Error,
            {
                Ok(SimScalar::from_num(v))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<SimScalar, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                if let Some(val) = seq.next_element::<f64>()? {
                    Ok(SimScalar::from_num(val))
                } else {
                    Err(serde::de::Error::invalid_length(0, &self))
                }
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_any(self)
            }
        }

        deserializer.deserialize_any(SimScalarVisitor)
    }
}

impl SimScalar {
    /// Zero value: 0.0
    pub const ZERO: Self = Self(I32F32::ZERO);

    /// Unit value: 1.0
    pub const ONE: Self = Self(I32F32::ONE);

    /// Half value: 0.5
    pub const HALF: Self = Self(I32F32::from_bits(1i64 << 31));

    /// Smallest positive representable delta: 2^-32
    pub const EPSILON: Self = Self(I32F32::from_bits(1));

    /// Maximum representable value: 2^31 - 1 + (2^32 - 1) / 2^32
    pub const MAX: Self = Self(I32F32::MAX);

    /// Minimum representable value: -2^31
    pub const MIN: Self = Self(I32F32::MIN);

    /// Archimedes' constant pi (π ≈ 3.14159265)
    pub const PI: Self = Self(I32F32::PI);

    /// Circle constant tau (2π ≈ 6.2831853)
    pub const TWO_PI: Self = Self(I32F32::TAU);

    /// Half pi (π / 2 ≈ 1.5707963)
    pub const HALF_PI: Self = Self(I32F32::FRAC_PI_2);

    /// Constructs a `SimScalar` from raw 64-bit integer bits.
    #[must_use]
    pub const fn from_raw(raw: i64) -> Self {
        Self(I32F32::from_bits(raw))
    }

    /// Returns the raw 64-bit integer representation.
    #[must_use]
    pub const fn raw(self) -> i64 {
        self.0.to_bits()
    }

    /// Constructs a `SimScalar` from a standard 32-bit signed integer.
    #[must_use]
    #[allow(clippy::cast_lossless)]
    pub const fn from_i32(val: i32) -> Self {
        Self(I32F32::from_bits((val as i64) << 32))
    }

    /// Constructs a `SimScalar` from any type implementing `ToFixed`.
    #[must_use]
    pub fn from_num<T: fixed::traits::ToFixed>(val: T) -> Self {
        Self(I32F32::from_num(val))
    }

    /// Converts scalar to `f32` at the visualization / snapshot boundary.
    ///
    /// # Safety & Charter Invariant
    /// Forbidden in simulation loop logic. Only permissible for render snapshots,
    /// debug logging, and user interface panels.
    #[must_use]
    pub fn to_f32(self) -> f32 {
        self.0.to_num::<f32>()
    }

    /// Converts `f32` to fixed-point for asset baking and data loading.
    ///
    /// # Safety & Charter Invariant
    /// Forbidden inside tick updates; use only during initial config/map parsing.
    #[must_use]
    pub fn from_f32_lossy(val: f32) -> Self {
        Self(I32F32::from_num(val))
    }

    /// Saturating addition.
    #[must_use]
    pub fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Saturating subtraction.
    #[must_use]
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// Saturating multiplication.
    #[must_use]
    pub fn saturating_mul(self, rhs: Self) -> Self {
        Self(self.0.saturating_mul(rhs.0))
    }

    /// Saturating division. Returns `MAX` or `MIN` on overflow or division by zero.
    #[must_use]
    pub fn saturating_div(self, rhs: Self) -> Self {
        Self(self.0.saturating_div(rhs.0))
    }

    /// Checked addition. Returns `None` if overflow occurred.
    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    /// Checked subtraction. Returns `None` if underflow/overflow occurred.
    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }

    /// Checked multiplication. Returns `None` if overflow occurred.
    #[must_use]
    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        self.0.checked_mul(rhs.0).map(Self)
    }

    /// Checked division. Returns `None` if division by zero or overflow occurred.
    #[must_use]
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        self.0.checked_div(rhs.0).map(Self)
    }

    /// Absolute value (saturating on `MIN`).
    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    /// Returns signum: -1.0 for negative, +1.0 for positive, 0.0 for zero.
    #[must_use]
    pub fn signum(self) -> Self {
        match self.0.cmp(&I32F32::ZERO) {
            std::cmp::Ordering::Greater => Self::ONE,
            std::cmp::Ordering::Less => -Self::ONE,
            std::cmp::Ordering::Equal => Self::ZERO,
        }
    }

    /// Clamps value between `min` and `max`.
    #[must_use]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        self.max(min).min(max)
    }

    /// Returns the minimum of two values.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        if self <= other {
            self
        } else {
            other
        }
    }

    /// Returns the maximum of two values.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }

    /// Deterministic bitwise integer square root algorithm on Q32.32.
    ///
    /// # Errors
    /// Returns `Err(MathError::NegativeSquareRoot)` if `self < 0`.
    // PERFORMANCE RATIONALE:
    // Why: Hardware float `f32::sqrt` / `f64::sqrt` can have slight rounding or transcendental divergence
    // across architectures (e.g. x86 SSE vs ARM NEON vs WASM), breaking simulation lockstep determinism.
    // What: We compute square root via exact integer arithmetic on the Q32.32 raw bit representation
    // shifted left by 32 bits into a 128-bit unsigned integer: sqrt(X * 2^32) = sqrt(x) * 2^32.
    // Trade-off: Uses 128-bit integer arithmetic which takes slightly more cycles than a single hardware
    // float instruction, in exchange for 100% bit-for-bit platform-independent determinism.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    pub fn sqrt(self) -> Result<Self, MathError> {
        let bits = self.0.to_bits();
        if bits < 0 {
            return Err(MathError::NegativeSquareRoot { value: bits });
        }
        if bits == 0 {
            return Ok(Self::ZERO);
        }
        let wide = u128::from(bits.unsigned_abs()) << 32;
        let root = isqrt_u128(wide);
        Ok(Self(I32F32::from_bits(root as i64)))
    }
}

/// Bitwise integer square root algorithm for `u128` values.
///
/// Compatible with MSRV 1.80.0 without hardware floating-point instructions.
const fn isqrt_u128(val: u128) -> u128 {
    if val <= 1 {
        return val;
    }
    let shift = val.ilog2() & !1;
    let mut one = 1u128 << shift;
    let mut num = val;
    let mut res = 0u128;
    while one != 0 {
        if num >= res + one {
            num -= res + one;
            res = (res >> 1) + one;
        } else {
            res >>= 1;
        }
        one >>= 2;
    }
    res
}

impl fmt::Debug for SimScalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SimScalar({:.4})", self.to_f32())
    }
}

impl fmt::Display for SimScalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.to_f32())
    }
}

impl Add for SimScalar {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Add<&SimScalar> for SimScalar {
    type Output = Self;
    fn add(self, rhs: &SimScalar) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Add<SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn add(self, rhs: SimScalar) -> SimScalar {
        SimScalar(self.0 + rhs.0)
    }
}

impl Add<&SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn add(self, rhs: &SimScalar) -> SimScalar {
        SimScalar(self.0 + rhs.0)
    }
}

impl AddAssign for SimScalar {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for SimScalar {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Sub<&SimScalar> for SimScalar {
    type Output = Self;
    fn sub(self, rhs: &SimScalar) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Sub<SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn sub(self, rhs: SimScalar) -> SimScalar {
        SimScalar(self.0 - rhs.0)
    }
}

impl Sub<&SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn sub(self, rhs: &SimScalar) -> SimScalar {
        SimScalar(self.0 - rhs.0)
    }
}

impl SubAssign for SimScalar {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul for SimScalar {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl Mul<&SimScalar> for SimScalar {
    type Output = Self;
    fn mul(self, rhs: &SimScalar) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl Mul<SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn mul(self, rhs: SimScalar) -> SimScalar {
        SimScalar(self.0 * rhs.0)
    }
}

impl Mul<&SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn mul(self, rhs: &SimScalar) -> SimScalar {
        SimScalar(self.0 * rhs.0)
    }
}

impl MulAssign for SimScalar {
    fn mul_assign(&mut self, rhs: Self) {
        self.0 *= rhs.0;
    }
}

impl Mul<i32> for SimScalar {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self {
        Self(self.0 * I32F32::from_num(rhs))
    }
}

impl Mul<SimScalar> for i32 {
    type Output = SimScalar;
    fn mul(self, rhs: SimScalar) -> SimScalar {
        SimScalar(I32F32::from_num(self) * rhs.0)
    }
}

impl Div for SimScalar {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        Self(self.0 / rhs.0)
    }
}

impl Div<&SimScalar> for SimScalar {
    type Output = Self;
    fn div(self, rhs: &SimScalar) -> Self {
        Self(self.0 / rhs.0)
    }
}

impl Div<SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn div(self, rhs: SimScalar) -> SimScalar {
        SimScalar(self.0 / rhs.0)
    }
}

impl Div<&SimScalar> for &SimScalar {
    type Output = SimScalar;
    fn div(self, rhs: &SimScalar) -> SimScalar {
        SimScalar(self.0 / rhs.0)
    }
}

impl DivAssign for SimScalar {
    fn div_assign(&mut self, rhs: Self) {
        self.0 /= rhs.0;
    }
}

impl Div<i32> for SimScalar {
    type Output = Self;
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / I32F32::from_num(rhs))
    }
}

impl Neg for SimScalar {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Neg for &SimScalar {
    type Output = SimScalar;
    fn neg(self) -> SimScalar {
        SimScalar(-self.0)
    }
}

/// 2D deterministic fixed-point vector representing planar position, velocity, or direction.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SimVec2 {
    /// X coordinate (East-West axis).
    pub x: SimScalar,
    /// Y coordinate (North-South axis).
    pub y: SimScalar,
}

impl SimVec2 {
    /// Zero vector: (0.0, 0.0)
    pub const ZERO: Self = Self::new(SimScalar::ZERO, SimScalar::ZERO);

    /// Unit X direction vector: (1.0, 0.0)
    pub const X_AXIS: Self = Self::new(SimScalar::ONE, SimScalar::ZERO);

    /// Unit Y direction vector: (0.0, 1.0)
    pub const Y_AXIS: Self = Self::new(SimScalar::ZERO, SimScalar::ONE);

    /// Constructs a vector with explicit components.
    #[must_use]
    pub const fn new(x: SimScalar, y: SimScalar) -> Self {
        Self { x, y }
    }

    /// Constructs a vector from integer coordinates.
    #[must_use]
    pub const fn from_i32(x: i32, y: i32) -> Self {
        Self {
            x: SimScalar::from_i32(x),
            y: SimScalar::from_i32(y),
        }
    }

    /// Computes the dot product: `x1 * x2 + y1 * y2`.
    #[must_use]
    pub fn dot(self, rhs: Self) -> SimScalar {
        self.x * rhs.x + self.y * rhs.y
    }

    /// Computes the 2D cross product (perp-dot product): `x1 * y2 - y1 * x2`.
    ///
    /// Essential for funnel string-pulling, triangle orientation, and turn steering.
    #[must_use]
    pub fn cross_2d(self, rhs: Self) -> SimScalar {
        self.x * rhs.y - self.y * rhs.x
    }

    /// Computes squared Euclidean length: `x^2 + y^2`.
    #[must_use]
    pub fn length_sq(self) -> SimScalar {
        self.x * self.x + self.y * self.y
    }

    /// Computes Euclidean length: `sqrt(x^2 + y^2)`.
    #[must_use]
    pub fn length(self) -> SimScalar {
        match self.length_sq().sqrt() {
            Ok(len) => len,
            Err(_) => SimScalar::ZERO,
        }
    }

    /// Computes squared Euclidean distance between two points.
    #[must_use]
    pub fn distance_sq(self, rhs: Self) -> SimScalar {
        (self - rhs).length_sq()
    }

    /// Computes Euclidean distance between two points.
    #[must_use]
    pub fn distance(self, rhs: Self) -> SimScalar {
        (self - rhs).length()
    }

    /// Normalizes vector to unit length.
    ///
    /// # Errors
    /// Returns `Err(MathError::ZeroLengthVector)` if length is below `SimScalar::EPSILON`.
    pub fn normalize(self) -> Result<Self, MathError> {
        let len = self.length();
        if len <= SimScalar::EPSILON {
            return Err(MathError::ZeroLengthVector);
        }
        Ok(Self {
            x: self.x / len,
            y: self.y / len,
        })
    }

    /// Normalizes vector or returns `SimVec2::ZERO` if length is below `SimScalar::EPSILON`.
    #[must_use]
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len <= SimScalar::EPSILON {
            Self::ZERO
        } else {
            Self {
                x: self.x / len,
                y: self.y / len,
            }
        }
    }

    /// Rotates vector counter-clockwise by `angle` using deterministic trigonometry.
    #[must_use]
    pub fn rotate_by(self, angle: SimAngle) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            x: self.x * cos - self.y * sin,
            y: self.x * sin + self.y * cos,
        }
    }

    /// Converts coordinates to `[f32; 2]` for render uniform buffer submission.
    #[must_use]
    pub fn to_f32_array(self) -> [f32; 2] {
        [self.x.to_f32(), self.y.to_f32()]
    }
}

impl fmt::Debug for SimVec2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SimVec2({:.4}, {:.4})", self.x.to_f32(), self.y.to_f32())
    }
}

impl fmt::Display for SimVec2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.4}, {:.4})", self.x.to_f32(), self.y.to_f32())
    }
}

impl Add for SimVec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl AddAssign for SimVec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for SimVec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl SubAssign for SimVec2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<SimScalar> for SimVec2 {
    type Output = Self;
    fn mul(self, rhs: SimScalar) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Mul<i32> for SimVec2 {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl MulAssign<SimScalar> for SimVec2 {
    fn mul_assign(&mut self, rhs: SimScalar) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl Div<SimScalar> for SimVec2 {
    type Output = Self;
    fn div(self, rhs: SimScalar) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl Div<i32> for SimVec2 {
    type Output = Self;
    fn div(self, rhs: i32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl DivAssign<SimScalar> for SimVec2 {
    fn div_assign(&mut self, rhs: SimScalar) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

impl Neg for SimVec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

/// Deterministic binary angle representing [0, 2π) mapped across the full range of `u16` (0..=65535).
///
/// Under this representation, modular circle wrapping is handled automatically by CPU unsigned integer
/// overflow arithmetic, guaranteeing zero rounding errors or branch mispredictions at 0/360 boundary.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[repr(transparent)]
pub struct SimAngle(pub u16);

impl SimAngle {
    /// 0 degrees / 0 radians.
    pub const ZERO: Self = Self(0);

    /// 90 degrees / π/2 radians.
    pub const DEG_90: Self = Self(16384);

    /// 180 degrees / π radians.
    pub const DEG_180: Self = Self(32768);

    /// π radians / 180 degrees.
    pub const PI: Self = Self::DEG_180;

    /// 270 degrees / 3π/2 radians.
    pub const DEG_270: Self = Self(49152);

    /// Constructs a `SimAngle` directly from raw binary units (0..=65535).
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the raw binary angle units (0..=65535).
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Converts degrees integer to binary angle with exact Euclidean modulo wrapping.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn from_degrees(deg: i32) -> Self {
        let wrapped = i64::from(deg.rem_euclid(360));
        Self(((wrapped * 65536) / 360) as u16)
    }

    /// Converts binary angle to floating-point degrees [0.0, 360.0) for debug inspection.
    #[must_use]
    pub fn to_degrees_f32(self) -> f32 {
        (f32::from(self.0) / 65536.0) * 360.0
    }

    /// Converts binary angle to floating-point radians [0.0, 2π) for render shaders.
    #[must_use]
    pub fn to_radians_f32(self) -> f32 {
        (f32::from(self.0) / 65536.0) * std::f32::consts::TAU
    }

    /// Shortest signed angular difference from `self` to `other` in range `[-32768, 32767]`.
    ///
    /// Positive values denote counter-clockwise rotation; negative values denote clockwise rotation.
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn diff(self, other: Self) -> i16 {
        other.0.wrapping_sub(self.0) as i16
    }

    /// Steps `self` towards `target` by at most `max_step` units along the shortest path.
    ///
    /// Snaps directly to `target` if the remaining angular distance is `<= max_step` to prevent oscillation.
    #[must_use]
    pub fn rotate_towards(self, target: Self, max_step: u16) -> Self {
        let d = self.diff(target);
        if d == 0 {
            return self;
        }
        let abs_d = d.unsigned_abs();
        if abs_d <= max_step {
            target
        } else if d > 0 {
            Self(self.0.wrapping_add(max_step))
        } else {
            Self(self.0.wrapping_sub(max_step))
        }
    }

    /// Computes deterministic sine using a 1024-entry quadrant lookup table with linear interpolation.
    // PERFORMANCE RATIONALE:
    // Why: Standard libc `f32::sin`/`f32::cos` can diverge between x86 and ARM platforms, breaking lockstep.
    // Computing trigonometric series via Taylor polynomial at runtime introduces high CPU instruction overhead.
    // What: We use a 1024-entry precomputed fixed-point Lookup Table (LUT) for the first quadrant [0, π/2],
    // exploiting 4-quadrant symmetry for `sin` and `cos`.
    // Trade-off: 4 KB of static read-only memory in exchange for O(1) branch-predictable lookups and
    // 100% bit-for-bit determinism across all targets.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn sin(self) -> SimScalar {
        let a = self.0;
        let (quad_angle, negative) = match a {
            0..=16384 => (a, false),
            16385..=32768 => (32768 - a, false),
            32769..=49152 => (a - 32768, true),
            49153..=65535 => ((65536u32 - u32::from(a)) as u16, true),
        };

        let idx = (quad_angle >> 4) as usize;
        let rem = u64::from(quad_angle & 0x0F);

        let y: u64 = if idx >= 1024 {
            1u64 << 32
        } else {
            let y0 = u64::from(SIN_TABLE[idx]);
            let y1 = if idx + 1 < 1024 {
                u64::from(SIN_TABLE[idx + 1])
            } else {
                1u64 << 32
            };
            let diff = y1 - y0;
            y0 + ((diff * rem) >> 4)
        };

        if negative {
            SimScalar::from_raw(-(y as i64))
        } else {
            SimScalar::from_raw(y as i64)
        }
    }

    /// Computes deterministic cosine: `cos(θ) = sin(θ + 90°)`.
    #[must_use]
    pub fn cos(self) -> SimScalar {
        Self(self.0.wrapping_add(16384)).sin()
    }

    /// Computes deterministic 4-quadrant arctangent `atan2(y, x)` in discrete binary angle units.
    ///
    /// Implemented via exact 14-step monotonic binary search on the trigonometric LUT dual.
    #[must_use]
    pub fn atan2(y: SimScalar, x: SimScalar) -> Self {
        let raw_x = x.raw();
        let raw_y = y.raw();

        if raw_x == 0 && raw_y == 0 {
            return Self::ZERO;
        }
        if raw_y == 0 {
            return if raw_x > 0 { Self::ZERO } else { Self::DEG_180 };
        }
        if raw_x == 0 {
            return if raw_y > 0 {
                Self::DEG_90
            } else {
                Self::DEG_270
            };
        }
        if raw_x.abs() == raw_y.abs() {
            let quad: u16 = 8192;
            return if raw_x > 0 && raw_y > 0 {
                Self(quad)
            } else if raw_x < 0 && raw_y > 0 {
                Self(32768 - quad)
            } else if raw_x < 0 && raw_y < 0 {
                Self(32768 + quad)
            } else {
                Self(quad.wrapping_neg())
            };
        }

        let abs_x = i128::from(raw_x.unsigned_abs());
        let abs_y = i128::from(raw_y.unsigned_abs());

        // Binary search on first quadrant [0, 16384]:
        let mut low: u16 = 0;
        let mut high: u16 = 16384;
        let mut best: u16 = 0;
        let mut min_err: i128 = i128::MAX;

        while low <= high {
            let mid = low + (high - low) / 2;
            let ang = Self(mid);
            let s = i128::from(ang.sin().raw());
            let c = i128::from(ang.cos().raw());
            // Cross product: abs_x * sin(mid) - abs_y * cos(mid)
            let val = abs_x * s - abs_y * c;
            let err = val.abs();
            if err < min_err {
                min_err = err;
                best = mid;
            }
            match val.cmp(&0) {
                std::cmp::Ordering::Less => {
                    low = mid.saturating_add(1);
                }
                std::cmp::Ordering::Greater => {
                    if mid == 0 {
                        break;
                    }
                    high = mid - 1;
                }
                std::cmp::Ordering::Equal => {
                    best = mid;
                    break;
                }
            }
        }

        let quad = best;
        if raw_x > 0 && raw_y > 0 {
            Self(quad)
        } else if raw_x < 0 && raw_y > 0 {
            Self(32768 - quad)
        } else if raw_x < 0 && raw_y < 0 {
            Self(32768 + quad)
        } else {
            Self(quad.wrapping_neg())
        }
    }
}

impl fmt::Debug for SimAngle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SimAngle({} [raw={:#06x}, {:.2}°])",
            self.0,
            self.0,
            self.to_degrees_f32()
        )
    }
}

impl fmt::Display for SimAngle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}°", self.to_degrees_f32())
    }
}

impl Add for SimAngle {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl AddAssign for SimAngle {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_add(rhs.0);
    }
}

impl Sub for SimAngle {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl SubAssign for SimAngle {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_sub(rhs.0);
    }
}

impl Neg for SimAngle {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_point_basic_arithmetic() {
        let x = SimScalar::from_i32(5);
        assert_eq!(x + SimScalar::ZERO, x);
        assert_eq!(x * SimScalar::ONE, x);
        assert_eq!(x - x, SimScalar::ZERO);

        let p1 = SimScalar::from_num(0.1f64);
        let p2 = SimScalar::from_num(0.2f64);
        let p3 = SimScalar::from_num(0.3f64);
        let diff = (p1 + p2 - p3).abs();
        assert!(diff <= SimScalar::EPSILON * 2);
    }

    #[test]
    fn test_saturating_bounds() {
        assert_eq!(
            SimScalar::MAX.saturating_add(SimScalar::ONE),
            SimScalar::MAX
        );
        assert_eq!(
            SimScalar::MIN.saturating_sub(SimScalar::ONE),
            SimScalar::MIN
        );
    }

    #[test]
    fn test_deterministic_sqrt() {
        assert_eq!(SimScalar::ZERO.sqrt().unwrap(), SimScalar::ZERO);
        assert_eq!(SimScalar::ONE.sqrt().unwrap(), SimScalar::ONE);
        assert_eq!(
            SimScalar::from_i32(4).sqrt().unwrap(),
            SimScalar::from_i32(2)
        );
        assert_eq!(
            SimScalar::from_i32(9).sqrt().unwrap(),
            SimScalar::from_i32(3)
        );
        assert_eq!(
            SimScalar::from_i32(100).sqrt().unwrap(),
            SimScalar::from_i32(10)
        );
        assert_eq!(
            SimScalar::from_i32(250_000).sqrt().unwrap(),
            SimScalar::from_i32(500)
        );

        let err = SimScalar::from_i32(-1).sqrt();
        assert!(matches!(err, Err(MathError::NegativeSquareRoot { .. })));
    }

    #[test]
    fn test_division_by_zero_handling() {
        assert_eq!(SimScalar::ONE.checked_div(SimScalar::ZERO), None);
        assert_eq!(SimVec2::ZERO.normalize_or_zero(), SimVec2::ZERO);
        assert!(matches!(
            SimVec2::ZERO.normalize(),
            Err(MathError::ZeroLengthVector)
        ));
    }

    #[test]
    fn test_simvec2_dot_and_cross() {
        let vx = SimVec2::X_AXIS;
        let vy = SimVec2::Y_AXIS;

        // Orthogonal
        assert_eq!(vx.dot(vy), SimScalar::ZERO);
        assert_eq!(vx.cross_2d(vy), SimScalar::ONE);

        // Parallel
        let v2x = vx * SimScalar::from_i32(2);
        let v3x = vx * SimScalar::from_i32(3);
        assert_eq!(v2x.dot(v3x), SimScalar::from_i32(6));
        assert_eq!(v2x.cross_2d(v3x), SimScalar::ZERO);

        // Antiparallel
        let neg_vx = -vx;
        assert_eq!(vx.dot(neg_vx), -SimScalar::ONE);
        assert_eq!(vx.cross_2d(neg_vx), SimScalar::ZERO);
    }

    #[test]
    fn test_simvec2_rotation() {
        let v = SimVec2::X_AXIS;

        // Rotate by 90 deg -> Y_AXIS
        let rot90 = v.rotate_by(SimAngle::DEG_90);
        assert!((rot90.x - SimScalar::ZERO).abs() < SimScalar::from_num(0.001f32));
        assert!((rot90.y - SimScalar::ONE).abs() < SimScalar::from_num(0.001f32));

        // Rotate by 180 deg -> -X_AXIS
        let rot180 = v.rotate_by(SimAngle::DEG_180);
        assert!((rot180.x - (-SimScalar::ONE)).abs() < SimScalar::from_num(0.001f32));
        assert!((rot180.y - SimScalar::ZERO).abs() < SimScalar::from_num(0.001f32));

        // Rotate by 360 deg -> original
        let rot360 = v.rotate_by(SimAngle::ZERO);
        assert_eq!(rot360, v);
    }

    #[test]
    fn test_simangle_wrapping() {
        assert_eq!(SimAngle(65535) + SimAngle(1), SimAngle(0));
        assert_eq!(SimAngle(0) - SimAngle(1), SimAngle(65535));
    }

    #[test]
    fn test_simangle_shortest_diff() {
        let a10 = SimAngle::from_degrees(10);
        let a20 = SimAngle::from_degrees(20);
        assert_eq!(a10.diff(a20), 1820);
        assert_eq!(a20.diff(a10), -1820);

        // Wrapping across 0 boundary
        let a355 = SimAngle(64627);
        let a5 = SimAngle(910);
        assert_eq!(a355.diff(a5), 1819);
        assert_eq!(a5.diff(a355), -1819);
    }

    #[test]
    fn test_simangle_rotate_towards() {
        let start = SimAngle(1000);
        let target = SimAngle(3000);
        let stepped = start.rotate_towards(target, 500);
        assert_eq!(stepped, SimAngle(1500));

        let close_target = SimAngle(1200);
        let snapped = start.rotate_towards(close_target, 500);
        assert_eq!(snapped, close_target);
    }

    #[test]
    fn test_trig_symmetry() {
        assert_eq!(SimAngle::ZERO.sin(), SimScalar::ZERO);
        assert_eq!(SimAngle::DEG_90.sin(), SimScalar::ONE);
        assert_eq!(SimAngle::DEG_180.sin(), SimScalar::ZERO);
        assert_eq!(SimAngle::DEG_270.sin(), -SimScalar::ONE);

        assert_eq!(SimAngle::ZERO.cos(), SimScalar::ONE);
        assert_eq!(SimAngle::DEG_90.cos(), SimScalar::ZERO);
        assert_eq!(SimAngle::DEG_180.cos(), -SimScalar::ONE);
        assert_eq!(SimAngle::DEG_270.cos(), SimScalar::ZERO);

        let max_err = SimScalar::from_num(0.001f32);
        for a in 0..=65535 {
            let ang = SimAngle(a);
            let s = ang.sin();
            let c = ang.cos();
            let pyth = s * s + c * c;
            let err = (pyth - SimScalar::ONE).abs();
            assert!(
                err < max_err,
                "Pythagorean error at angle {a}: {err} exceeds {max_err}"
            );
        }
    }

    #[test]
    fn test_atan2_quadrants() {
        assert_eq!(
            SimAngle::atan2(SimScalar::ZERO, SimScalar::ONE),
            SimAngle::ZERO
        );
        assert_eq!(
            SimAngle::atan2(SimScalar::ONE, SimScalar::ZERO),
            SimAngle::DEG_90
        );
        assert_eq!(
            SimAngle::atan2(SimScalar::ZERO, -SimScalar::ONE),
            SimAngle::DEG_180
        );
        assert_eq!(
            SimAngle::atan2(-SimScalar::ONE, SimScalar::ZERO),
            SimAngle::DEG_270
        );
        assert_eq!(
            SimAngle::atan2(SimScalar::ONE, SimScalar::ONE),
            SimAngle(8192)
        );
    }
}
