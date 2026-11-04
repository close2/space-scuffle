//! The flipbook clock: one presentation time shared by every flipbook, and the integer function
//! that turns it into an atlas layer on the GPU and on the CPU alike.
//!
//! # Responsibility
//! This module owns how a bubble's flipbook advances with time (plan
//! `docs/plans/2026-11-04-battle-flipbook-animation.md`, decisions D-1 to D-3 and D-6):
//! - [`FlipbookClock`] is the presented time in the main world, in [`CLOCK_UNITS_PER_TICK`] units
//!   per simulation tick.
//! - [`FLIPBOOK_CLOCK_BUFFER`] is one 16-byte storage buffer, bound at `@group(2) @binding(3)` of
//!   every [`BubbleDomeMaterial`](crate::bubble_shader::BubbleDomeMaterial). During extraction,
//!   [`upload_flipbook_clock_system`] writes the clock into it in place.
//! - [`FlipbookLanes`] are the per-bubble values packed into `BubbleUniformGpu`. They describe what
//!   the bubble is doing (which slice, how long one cycle lasts, where the cycle starts) and never
//!   the current time, so a material changes only when the action changes.
//! - [`flipbook_sample`] is the Rust mirror of `flipbook_select` in `assets/shaders/bubble_dome.wgsl`.
//!
//! # How it fits
//! The main world sets the clock once per frame; the GPU computes each bubble's layer from the
//! clock and the bubble's lanes. Nothing per bubble is written while time passes, so the asset
//! pipeline prepares no material and no buffer for animation alone (plan section 2.4).
//!
//! # Invariants
//! - Frame selection is integer arithmetic on `u32` with wrapping addition, subtraction and
//!   multiplication, the semantics of WGSL's `u32`. The Rust mirror and every GPU driver therefore
//!   pick the same layer at every frame boundary.
//! - The clock wraps after 2³² units, 46.6 hours of game time at 25 ticks per second. A loop
//!   glitches once at that wrap; this is documented, not handled.
//! - The clock buffer asset is created once at plugin build and never modified, so no prepared
//!   material ever holds a stale buffer (a modified `ShaderStorageBuffer` gets a new GPU buffer).
//! - The lanes constructors guarantee `count >= 1`, `period >= 1`, `offset < period` and
//!   `period · count < 2³²`, so the arithmetic of a constructed lane never wraps except in the clock.

use bevy::prelude::*;
use bevy::render::render_asset::{RenderAssetUsages, RenderAssets};
use bevy::render::render_resource::BufferUsages;
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::{GpuShaderStorageBuffer, ShaderStorageBuffer};
use bevy::render::{Extract, ExtractSchedule, RenderApp};

/// Clock units in one simulation tick: the clock counts 1/1024 tick, 39.06 µs of game time at
/// 25 ticks per second.
pub const CLOCK_UNITS_PER_TICK: u32 = 1024;

/// Fixed id of the shared clock storage buffer that every bubble material binds.
///
/// The handle is weak: the asset is inserted under this id at plugin build and is never removed,
/// because no strong handle exists whose drop could remove it.
pub const FLIPBOOK_CLOCK_BUFFER: Handle<ShaderStorageBuffer> =
    Handle::weak_from_u128(0x0b5c_7a1e_f11b_0c10_c4b0_0f5e_ed10_2026);

/// Presented time of the flipbooks, in [`CLOCK_UNITS_PER_TICK`] units per simulation tick.
///
/// The battle writes the presented simulation instant here; scenes without a battle write their
/// own elapsed time in the same unit. The value wraps (see the module invariants).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlipbookClock {
    /// Presented time in clock units.
    pub now: u32,
}

mod gpu_layout {
    //! Holds one `ShaderType` struct, so the lint expectation below covers nothing else.
    #![expect(
        dead_code,
        reason = "the ShaderType derive emits layout checks nothing calls"
    )]

    use bevy::render::render_resource::ShaderType;

    /// Contents of the clock storage buffer, matching `struct FlipbookClock` in `bubble_dome.wgsl`.
    // PERFORMANCE RATIONALE:
    // Why: The buffer is written with `Queue::write_buffer` from these exact bytes, bypassing
    // `encase`, so its layout must be fixed by the type itself.
    // What: Four `u32` lanes, 16 bytes, `repr(C)`: the clock and three explicit padding lanes
    // that round the buffer up to one 16-byte row.
    // Trade-off: Twelve unused bytes in exchange for a layout that `bytemuck` and `encase`
    // provably agree on (tested below).
    #[repr(C)]
    #[derive(
        Debug, Clone, Copy, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable, ShaderType,
    )]
    pub struct FlipbookClockGpu {
        /// Presented time in clock units.
        pub now: u32,
        /// Padding to 16 bytes; always zero.
        pub pad0: u32,
        /// Padding to 16 bytes; always zero.
        pub pad1: u32,
        /// Padding to 16 bytes; always zero.
        pub pad2: u32,
    }
}
pub use gpu_layout::FlipbookClockGpu;

impl FlipbookClockGpu {
    /// The buffer contents that present time `now`, in clock units.
    #[must_use]
    pub const fn new(now: u32) -> Self {
        Self {
            now,
            pad0: 0,
            pad1: 0,
            pad2: 0,
        }
    }
}

/// How a bubble's lanes turn the clock into a layer. Stored in bits 0 and 1 of `anim_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FlipbookMode {
    /// Always the layer `base`.
    Fixed = 0,
    /// Cycles through `count` layers once per `period`.
    Loop = 1,
    /// Plays the `count` layers once over `period`, then holds the last.
    OnceHold = 2,
    /// Plays the `count` layers once over `period`, then shows `base` again.
    Once = 3,
}

impl FlipbookMode {
    /// Bits of `anim_mode` that hold the mode.
    pub const MASK: u32 = 0b11;

    /// The mode stored in the low bits of `anim_mode`. Every value maps to a mode.
    #[must_use]
    pub const fn from_bits(anim_mode: u32) -> Self {
        match anim_mode & Self::MASK {
            0 => Self::Fixed,
            1 => Self::Loop,
            2 => Self::OnceHold,
            _ => Self::Once,
        }
    }
}

/// Bit of `anim_mode` that asks for blending between consecutive layers of a loop. The shader
/// blends only when the material's `FLAG_FRAME_BLEND` feature flag is also set.
pub const ANIM_BLEND_BIT: u32 = 1 << 2;

/// Why a slice cannot be played by the flipbook clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FlipbookLanesError {
    /// The slice has no frames.
    #[error("a flipbook slice needs at least one frame")]
    NoFrames,
    /// The slice lasts no time.
    #[error("a flipbook slice needs a period of at least one clock unit")]
    ZeroPeriod,
    /// `period · count` does not fit in `u32`, so the integer frame function would wrap.
    #[error(
        "a period of {period} clock units over {count} frames exceeds the u32 frame arithmetic"
    )]
    TooLong {
        /// Period in clock units.
        period: u32,
        /// Frames in the slice.
        count: u32,
    },
    /// The slice's last layer is beyond `u32`.
    #[error("a slice of {count} frames starting at layer {base} ends beyond u32")]
    LayerOverflow {
        /// First layer of the slice.
        base: u32,
        /// Frames in the slice.
        count: u32,
    },
}

/// A validated run of atlas layers and the clock time one pass over them takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlipbookSpan {
    base: u32,
    count: u32,
    period: u32,
}

impl FlipbookSpan {
    /// The span of `count` layers from layer `base`, one pass lasting `period` clock units.
    ///
    /// # Errors
    /// [`FlipbookLanesError`] when `count` or `period` is zero, when `period · count` does not fit
    /// in `u32` (a slice of 128 frames may last up to 32 767 ticks), or when the last layer does
    /// not fit in `u32`.
    pub fn new(base: u32, count: u32, period: u32) -> Result<Self, FlipbookLanesError> {
        if count == 0 {
            return Err(FlipbookLanesError::NoFrames);
        }
        if period == 0 {
            return Err(FlipbookLanesError::ZeroPeriod);
        }
        if period.checked_mul(count).is_none() {
            return Err(FlipbookLanesError::TooLong { period, count });
        }
        if base.checked_add(count - 1).is_none() {
            return Err(FlipbookLanesError::LayerOverflow { base, count });
        }
        Ok(Self {
            base,
            count,
            period,
        })
    }

    /// First atlas layer of the span.
    #[must_use]
    pub const fn base(&self) -> u32 {
        self.base
    }

    /// Layers in the span (at least one).
    #[must_use]
    pub const fn count(&self) -> u32 {
        self.count
    }

    /// Clock units of one pass over the span (at least one).
    #[must_use]
    pub const fn period(&self) -> u32 {
        self.period
    }
}

/// The per-bubble flipbook lanes of `BubbleUniformGpu`, which change only with the action.
///
/// Every field is a raw uniform lane, so any value is a valid input to [`flipbook_sample`]; the
/// constructors guarantee the ranges stated in the module invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlipbookLanes {
    /// Clock time at which `elapsed = 0`, in clock units (`anim_origin`).
    pub origin: u32,
    /// First atlas layer of the slice (`frame_base`).
    pub base: u32,
    /// Clock units of one pass over the slice (`anim_period`).
    pub period: u32,
    /// Layers in the slice (`frame_count`).
    pub count: u32,
    /// [`FlipbookMode`] in bits 0 and 1, [`ANIM_BLEND_BIT`] in bit 2 (`anim_mode`).
    pub mode: u32,
    /// Phase added to a loop's elapsed time, in clock units, below `period` (`anim_offset`).
    pub offset: u32,
}

impl FlipbookLanes {
    /// Lanes that always show atlas layer `layer`.
    #[must_use]
    pub const fn fixed(layer: u32) -> Self {
        Self {
            origin: 0,
            base: layer,
            period: 0,
            count: 0,
            mode: FlipbookMode::Fixed as u32,
            offset: 0,
        }
    }

    /// Lanes that loop over `span` with its first layer starting at clock time `phase_zero_at`.
    ///
    /// The lanes are canonical: the origin is 0 and the start is folded into the offset
    /// `(−phase_zero_at) mod period`, so two calls for the same loop phase give equal lanes even
    /// when their starts differ by whole periods. `blend` asks for frame blending (D-6).
    #[must_use]
    pub const fn looping(span: FlipbookSpan, phase_zero_at: u32, blend: bool) -> Self {
        let period = span.period;
        let offset = (period - phase_zero_at % period) % period;
        Self {
            origin: 0,
            base: span.base,
            period,
            count: span.count,
            mode: FlipbookMode::Loop as u32 | if blend { ANIM_BLEND_BIT } else { 0 },
            offset,
        }
    }

    /// Lanes that play `span` once from clock time `started_at`, then hold the last layer
    /// (`hold_last`) or return to the first.
    #[must_use]
    pub const fn once(span: FlipbookSpan, started_at: u32, hold_last: bool) -> Self {
        let mode = if hold_last {
            FlipbookMode::OnceHold
        } else {
            FlipbookMode::Once
        };
        Self {
            origin: started_at,
            base: span.base,
            period: span.period,
            count: span.count,
            mode: mode as u32,
            offset: 0,
        }
    }
}

/// The layers and blend weight that a bubble shows at one clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlipbookSample {
    /// Atlas layer shown.
    pub layer: u32,
    /// Layer blended in: the next layer of a blending loop, else `layer`.
    pub next_layer: u32,
    /// Numerator of the weight of `next_layer`; zero when not blending.
    pub blend_numerator: u32,
    /// Denominator of the weight of `next_layer` (the loop's period); zero when not blending.
    pub blend_denominator: u32,
}

impl FlipbookSample {
    /// Weight of `next_layer` in `[0, 1)`, converted to `f32` once, as the shader does.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "the shader performs the same u32 to f32 conversion; parity, not precision, matters"
    )]
    pub fn blend_weight(&self) -> f32 {
        if self.blend_denominator == 0 {
            0.0
        } else {
            self.blend_numerator as f32 / self.blend_denominator as f32
        }
    }
}

/// The layers a bubble with `lanes` shows at clock time `now`: the Rust mirror of
/// `flipbook_select` in `bubble_dome.wgsl`, operation for operation.
///
/// With `e = now −% origin` (wrapping):
///
/// | mode | layer |
/// |---|---|
/// | `Fixed` | `base` |
/// | `Loop` | `base + (((e % period) + offset) % period) · count / period` |
/// | `OnceHold` | `base + min(e, period − 1) · count / period` |
/// | `Once` | as `OnceHold` while `e < period`, then `base` |
///
/// A loop with [`ANIM_BLEND_BIT`] also returns the next layer of the slice (the first after the
/// last) and the blend weight `((phase · count) % period) / period`. Lanes with no period or no
/// frames show `base`. Every operation is the wrapping `u32` arithmetic of WGSL.
#[must_use]
pub const fn flipbook_sample(now: u32, lanes: &FlipbookLanes) -> FlipbookSample {
    let still = FlipbookSample {
        layer: lanes.base,
        next_layer: lanes.base,
        blend_numerator: 0,
        blend_denominator: 0,
    };
    let (period, count) = (lanes.period, lanes.count);
    let mode = FlipbookMode::from_bits(lanes.mode);
    if matches!(mode, FlipbookMode::Fixed) || period == 0 || count == 0 {
        return still;
    }
    let elapsed = now.wrapping_sub(lanes.origin);
    match mode {
        FlipbookMode::Loop => {
            let phase = (elapsed % period).wrapping_add(lanes.offset) % period;
            let scaled = phase.wrapping_mul(count);
            let step = scaled / period;
            let layer = lanes.base.wrapping_add(step);
            if lanes.mode & ANIM_BLEND_BIT == 0 {
                return FlipbookSample {
                    layer,
                    next_layer: layer,
                    ..still
                };
            }
            FlipbookSample {
                layer,
                next_layer: lanes.base.wrapping_add(step.wrapping_add(1) % count),
                blend_numerator: scaled % period,
                blend_denominator: period,
            }
        }
        FlipbookMode::Once if elapsed >= period => still,
        FlipbookMode::Fixed | FlipbookMode::OnceHold | FlipbookMode::Once => {
            let clamped = if elapsed < period - 1 {
                elapsed
            } else {
                period - 1
            };
            let layer = lanes
                .base
                .wrapping_add(clamped.wrapping_mul(count) / period);
            FlipbookSample {
                layer,
                next_layer: layer,
                ..still
            }
        }
    }
}

/// The atlas layer a bubble with `lanes` shows at clock time `now` (see [`flipbook_sample`]).
#[must_use]
pub const fn flipbook_layer(now: u32, lanes: &FlipbookLanes) -> u32 {
    flipbook_sample(now, lanes).layer
}

/// Registers the clock resource, the shared clock buffer and its upload during extraction.
///
/// Added by [`BubbleDomeMaterialPlugin`](crate::bubble_shader::BubbleDomeMaterialPlugin), so no
/// app can draw a bubble material without its clock binding.
pub struct FlipbookClockPlugin;

impl Plugin for FlipbookClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlipbookClock>();
        // A headless app has no storage buffer assets and nothing to draw.
        if let Some(mut buffers) = app
            .world_mut()
            .get_resource_mut::<Assets<ShaderStorageBuffer>>()
        {
            buffers.insert(FLIPBOOK_CLOCK_BUFFER.id(), clock_buffer_asset());
        }
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, upload_flipbook_clock_system);
        }
    }
}

/// The clock buffer asset: the default clock, writable in place by the render queue.
fn clock_buffer_asset() -> ShaderStorageBuffer {
    let initial = FlipbookClockGpu::new(FlipbookClock::default().now);
    let mut buffer =
        ShaderStorageBuffer::new(bytemuck::bytes_of(&initial), RenderAssetUsages::default());
    buffer.buffer_description.label = Some("flipbook_clock");
    buffer.buffer_description.usage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
    buffer
}

/// Writes the main world's [`FlipbookClock`] into the shared clock buffer when it changed.
///
/// Runs in the render app's `ExtractSchedule`. Until the buffer is prepared it retries every
/// frame; the buffer starts with the default clock, so nothing is lost meanwhile.
// PERFORMANCE RATIONALE:
// Why: Modifying the `ShaderStorageBuffer` asset would create a new GPU buffer, and every bubble
// material prepared earlier would keep binding the old one (plan section 2.4).
// What: Extraction writes the 16 bytes straight into the existing buffer with
// `Queue::write_buffer`, at most once per frame and only when the clock moved.
// Trade-off: The asset's CPU copy keeps the initial clock and no longer describes the GPU buffer.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub(crate) fn upload_flipbook_clock_system(
    clock: Extract<'_, '_, Res<'_, FlipbookClock>>,
    buffers: Res<'_, RenderAssets<GpuShaderStorageBuffer>>,
    queue: Res<'_, RenderQueue>,
    mut uploaded: Local<'_, FlipbookClock>,
) {
    if **clock == *uploaded {
        return;
    }
    // The buffer is prepared after the first extraction, so absence here is a normal wait.
    let Some(gpu) = buffers.get(FLIPBOOK_CLOCK_BUFFER.id()) else {
        return;
    };
    queue.write_buffer(
        &gpu.buffer,
        0,
        bytemuck::bytes_of(&FlipbookClockGpu::new(clock.now)),
    );
    *uploaded = **clock;
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed test is a panic; each message states the invariant it relies on"
)]
mod tests {
    use super::*;
    use bevy::render::render_resource::encase::StorageBuffer;

    /// Ten layers from layer 30, one pass over 4 ticks (a 10-frame slice of 160 ms).
    fn span() -> FlipbookSpan {
        FlipbookSpan::new(30, 10, 4 * CLOCK_UNITS_PER_TICK)
            .expect("invariant: a 10-frame, 4-tick span is valid")
    }

    /// The first clock time that shows step `k` of `count` over `period`: `⌈k · period / count⌉`.
    fn boundary(k: u32, count: u32, period: u32) -> u32 {
        (k * period).div_ceil(count)
    }

    #[test]
    fn fixed_lanes_show_their_layer_at_every_time() {
        let lanes = FlipbookLanes::fixed(17);
        for now in [0, 1, 1023, 1024, u32::MAX / 2, u32::MAX] {
            assert_eq!(flipbook_layer(now, &lanes), 17);
        }
    }

    #[test]
    fn a_loop_steps_exactly_at_every_frame_boundary_and_wraps() {
        let span = span();
        let lanes = FlipbookLanes::looping(span, 0, false);
        assert_eq!(flipbook_layer(0, &lanes), 30, "e = 0 shows the first layer");
        for k in 1..span.count() {
            let at = boundary(k, span.count(), span.period());
            assert_eq!(flipbook_layer(at, &lanes), 30 + k, "boundary {k}");
            assert_eq!(
                flipbook_layer(at - 1, &lanes),
                30 + k - 1,
                "before boundary {k}"
            );
        }
        assert_eq!(flipbook_layer(span.period() - 1, &lanes), 39, "period − 1");
        assert_eq!(flipbook_layer(span.period(), &lanes), 30, "the loop wraps");
        assert_eq!(flipbook_layer(3 * span.period() + 1, &lanes), 30);
    }

    #[test]
    fn a_loop_survives_the_wrap_of_elapsed_time() {
        let span = span();
        // An origin after `now` makes `e = now −% origin` wrap; the arithmetic must not panic
        // and must match the shader's modular result.
        let lanes = FlipbookLanes {
            origin: 100,
            ..FlipbookLanes::looping(span, 0, false)
        };
        let e = 50u32.wrapping_sub(100);
        let expected = 30 + (e % span.period()) * span.count() / span.period();
        assert_eq!(flipbook_layer(50, &lanes), expected);
        // The clock itself wrapping: u32::MAX then 0 are consecutive units.
        let lanes = FlipbookLanes::looping(span, 0, false);
        let before = flipbook_layer(u32::MAX, &lanes);
        let after = flipbook_layer(0, &lanes);
        assert!((30..40).contains(&before) && after == 30);
    }

    #[test]
    fn the_canonical_loop_offset_matches_an_origin_at_the_start() {
        let span = span();
        for since in [0, 1, 1023, 4095, 4096, 77_777, u32::MAX - 5000] {
            let canonical = FlipbookLanes::looping(span, since, false);
            assert_eq!(canonical.origin, 0);
            assert!(canonical.offset < span.period());
            let from_origin = FlipbookLanes {
                origin: since,
                offset: 0,
                ..canonical
            };
            for step in 0..3 * span.period() {
                let now = since.wrapping_add(step);
                assert_eq!(
                    flipbook_layer(now, &canonical),
                    flipbook_layer(now, &from_origin),
                    "since {since}, step {step}"
                );
            }
        }
        // The same phase a whole number of periods later gives equal lanes, so a steady
        // repetition changes no material.
        assert_eq!(
            FlipbookLanes::looping(span, 9000, true),
            FlipbookLanes::looping(span, 9000 + 7 * span.period(), true)
        );
    }

    #[test]
    fn a_held_one_shot_stops_on_its_last_layer() {
        let span = span();
        let lanes = FlipbookLanes::once(span, 5000, true);
        assert_eq!(flipbook_layer(5000, &lanes), 30, "e = 0");
        for k in 1..span.count() {
            let at = 5000 + boundary(k, span.count(), span.period());
            assert_eq!(flipbook_layer(at, &lanes), 30 + k);
            assert_eq!(flipbook_layer(at - 1, &lanes), 30 + k - 1);
        }
        assert_eq!(flipbook_layer(5000 + span.period() - 1, &lanes), 39);
        assert_eq!(flipbook_layer(5000 + span.period(), &lanes), 39);
        assert_eq!(flipbook_layer(5000 + 1_000_000, &lanes), 39);
    }

    #[test]
    fn an_unheld_one_shot_returns_to_its_first_layer() {
        let span = span();
        let lanes = FlipbookLanes::once(span, 5000, false);
        assert_eq!(flipbook_layer(5000, &lanes), 30);
        assert_eq!(flipbook_layer(5000 + span.period() - 1, &lanes), 39);
        assert_eq!(flipbook_layer(5000 + span.period(), &lanes), 30);
        assert_eq!(flipbook_layer(5000 + 1_000_000, &lanes), 30);
    }

    #[test]
    fn the_longest_slice_at_the_limit_steps_exactly() {
        // 128 frames over 60 s of game time, the limit the plan states, is well inside u32.
        let sixty_seconds = 60 * 25 * CLOCK_UNITS_PER_TICK;
        let span = FlipbookSpan::new(0, 128, sixty_seconds).expect("invariant: within the limit");
        let lanes = FlipbookLanes::looping(span, 0, true);
        for k in 1..128 {
            let at = boundary(k, 128, sixty_seconds);
            assert_eq!(flipbook_layer(at, &lanes), k);
            assert_eq!(flipbook_layer(at - 1, &lanes), k - 1);
        }
        // The largest period for 128 frames is still accepted; one more unit is refused.
        let largest = u32::MAX / 128;
        let span = FlipbookSpan::new(0, 128, largest).expect("invariant: period · 128 fits u32");
        let lanes = FlipbookLanes::looping(span, 0, false);
        assert_eq!(flipbook_layer(largest - 1, &lanes), 127);
        assert_eq!(
            FlipbookSpan::new(0, 128, largest + 1),
            Err(FlipbookLanesError::TooLong {
                period: largest + 1,
                count: 128
            })
        );
    }

    #[test]
    fn invalid_spans_are_refused() {
        assert_eq!(
            FlipbookSpan::new(0, 0, 10),
            Err(FlipbookLanesError::NoFrames)
        );
        assert_eq!(
            FlipbookSpan::new(0, 4, 0),
            Err(FlipbookLanesError::ZeroPeriod)
        );
        assert_eq!(
            FlipbookSpan::new(u32::MAX, 2, 10),
            Err(FlipbookLanesError::LayerOverflow {
                base: u32::MAX,
                count: 2
            })
        );
    }

    #[test]
    fn degenerate_raw_lanes_show_the_base_layer() {
        // Raw lanes read back from a uniform need not come from a constructor.
        for mode in [
            FlipbookMode::Loop,
            FlipbookMode::OnceHold,
            FlipbookMode::Once,
        ] {
            let no_period = FlipbookLanes {
                origin: 0,
                base: 3,
                period: 0,
                count: 4,
                mode: mode as u32,
                offset: 0,
            };
            assert_eq!(flipbook_layer(99, &no_period), 3);
            let no_frames = FlipbookLanes {
                period: 10,
                count: 0,
                ..no_period
            };
            assert_eq!(flipbook_layer(99, &no_frames), 3);
        }
    }

    #[test]
    fn blending_mixes_toward_the_next_layer_and_wraps_to_the_first() {
        let span = span();
        let lanes = FlipbookLanes::looping(span, 0, true);
        let sample = flipbook_sample(0, &lanes);
        assert_eq!((sample.layer, sample.next_layer), (30, 31));
        assert_eq!(sample.blend_weight(), 0.0);
        // Halfway through step 0: (e · count) % period over period.
        let half = span.period() / span.count() / 2;
        let sample = flipbook_sample(half, &lanes);
        assert_eq!(sample.blend_numerator, half * span.count() % span.period());
        assert_eq!(sample.blend_denominator, span.period());
        // The last layer blends into the first.
        let sample = flipbook_sample(span.period() - 1, &lanes);
        assert_eq!((sample.layer, sample.next_layer), (39, 30));
        assert!(sample.blend_weight() > 0.9 && sample.blend_weight() < 1.0);
        // Without the blend bit, and in hold or fixed modes, nothing is blended.
        for lanes in [
            FlipbookLanes::looping(span, 0, false),
            FlipbookLanes::once(span, 0, true),
            FlipbookLanes::fixed(4),
        ] {
            let sample = flipbook_sample(half, &lanes);
            assert_eq!(sample.next_layer, sample.layer);
            assert_eq!(sample.blend_numerator, 0);
            assert_eq!(sample.blend_weight(), 0.0);
        }
    }

    #[test]
    fn every_mode_bit_pattern_decodes() {
        assert_eq!(FlipbookMode::from_bits(0), FlipbookMode::Fixed);
        assert_eq!(
            FlipbookMode::from_bits(1 | ANIM_BLEND_BIT),
            FlipbookMode::Loop
        );
        assert_eq!(FlipbookMode::from_bits(2), FlipbookMode::OnceHold);
        assert_eq!(FlipbookMode::from_bits(u32::MAX), FlipbookMode::Once);
    }

    /// The upload writes `bytemuck` bytes into a buffer the shader reads with `encase`'s storage
    /// layout, so the two encodings must agree byte for byte.
    #[test]
    fn the_clock_bytes_match_the_storage_layout() {
        let clock = FlipbookClockGpu::new(0x0102_0304);
        assert_eq!(size_of::<FlipbookClockGpu>(), 16);
        let mut encoded = StorageBuffer::new(Vec::<u8>::new());
        encoded
            .write(&clock)
            .expect("invariant: a 16-byte struct encodes into a growable vector");
        assert_eq!(encoded.into_inner(), bytemuck::bytes_of(&clock));
    }

    /// D-3 verify step: the buffer asset inserted under the weak handle has no strong handle,
    /// and must survive the asset system's handle tracking for the life of the app.
    #[test]
    fn the_clock_buffer_survives_without_a_strong_handle() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<ShaderStorageBuffer>()
            .add_plugins(FlipbookClockPlugin);
        for _ in 0..5 {
            app.update();
        }
        let buffers = app.world().resource::<Assets<ShaderStorageBuffer>>();
        let buffer = buffers
            .get(FLIPBOOK_CLOCK_BUFFER.id())
            .expect("the clock buffer outlives its weak handle");
        assert_eq!(
            buffer.data.as_deref(),
            Some(bytemuck::bytes_of(&FlipbookClockGpu::new(0)))
        );
        assert!(buffer
            .buffer_description
            .usage
            .contains(BufferUsages::STORAGE | BufferUsages::COPY_DST));
        assert_eq!(app.world().resource::<FlipbookClock>().now, 0);
    }
}
