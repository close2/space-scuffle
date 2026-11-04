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
//! - The battle's side (WP5, D-5 and D-8): [`update_flipbook_clock_system`] is the only writer of
//!   the clock, [`displayed_action`] says what a bubble shows, [`FlipbookPhase`] remembers since
//!   when, and [`battle_uniform`] packs a live bubble's lanes from its presented record.
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
//! - In the battle the clock is presented simulation time: a paused battle's frames leave it
//!   unchanged, and it advances by game time at every battle speed. It depends on ticks only, so
//!   a capture that steps one tick per frame is reproducible.
//! - A battle bubble's lanes depend on its presented record and its [`FlipbookPhase`] only, never
//!   on the clock: Attack and Constructing lanes come from the record alone, and a free loop's
//!   lanes are constant while its action lasts.
//! - The lanes constructors guarantee `count >= 1`, `period >= 1`, `offset < period` and
//!   `period · count < 2³²`, so the arithmetic of a constructed lane never wraps except in the clock.

use std::time::Duration;

use asset_baker::manifest::{ActionSlice, FlipbookAtlasMeta};
use bevy::prelude::*;
use bevy::render::render_asset::{RenderAssetUsages, RenderAssets};
use bevy::render::render_resource::BufferUsages;
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::{GpuShaderStorageBuffer, ShaderStorageBuffer};
use bevy::render::{Extract, ExtractSchedule, RenderApp};
use sim_types::{AnimState, BubbleRenderInstance, Tick};

use crate::battle_pipeline::session::BattleSession;
use crate::bubble_shader::BubbleUniformGpu;
use crate::lab_ui::{LabState, LabTab};
use crate::player_presentation::PlayerStyle;

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

// --- The battle's clock and display rules (WP5, D-1, D-5, D-8) -----------------------------------

/// Simulation ticks per second of game time (the 25 Hz contract of `battle_pipeline::clock`).
const TICKS_PER_SECOND: u64 = 25;

/// Clock units in one second of game time: 25 ticks of [`CLOCK_UNITS_PER_TICK`].
const CLOCK_UNITS_PER_SECOND: u64 = TICKS_PER_SECOND * CLOCK_UNITS_PER_TICK as u64;

/// The clock time of a presented battle instant: `tick · 1024 + ⌊fraction · 1024⌋`, wrapping.
///
/// `tick` is the presented record's simulation tick and `fraction` the interpolation towards the
/// next one, in `[0, 1)` (anything else is clamped into it). The result wraps with the clock.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "1024 is exact in f32, and the sub-tick part is clamped to 0..=1023 before the \
              conversion"
)]
pub fn presented_clock(tick: Tick, fraction: f32) -> u32 {
    let units = CLOCK_UNITS_PER_TICK as f32;
    let sub_tick = (fraction * units).floor().clamp(0.0, units - 1.0) as u32;
    tick.0
        .wrapping_mul(CLOCK_UNITS_PER_TICK)
        .wrapping_add(sub_tick)
}

/// The clock time of a scene without a battle, from Bevy's elapsed time: 25 · 1024 units per
/// second, wrapping (D-1).
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the clock keeps the low 32 bits by design: it wraps after 46.6 hours"
)]
pub fn elapsed_clock(elapsed: Duration) -> u32 {
    let units = elapsed.as_nanos() * u128::from(CLOCK_UNITS_PER_SECOND) / 1_000_000_000;
    units as u32
}

/// The single writer of [`FlipbookClock`] (D-8).
///
/// On the Live Battle tab the clock is the presented simulation instant
/// ([`BattleFrameState::presented_instant`](crate::battle_pipeline::timeline::BattleFrameState::presented_instant)):
/// it stands still while the battle is paused and runs at the battle's speed. Until the
/// session is ready it keeps its value. On every other tab it is Bevy's elapsed time.
///
/// Runs in `BattleSet::PreparePresentation` after the tick driver and before
/// `sync_live_battle_system`, so a frame's lanes and its clock describe the same instant.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn update_flipbook_clock_system(
    lab: Res<'_, LabState>,
    session: Res<'_, BattleSession>,
    time: Res<'_, Time>,
    mut clock: ResMut<'_, FlipbookClock>,
) {
    let now = if lab.current_tab == LabTab::LiveBattle {
        if !session.is_ready() {
            return;
        }
        let (tick, fraction) = session.frames.presented_instant();
        presented_clock(tick, fraction)
    } else {
        elapsed_clock(time.elapsed())
    };
    clock.set_if_neq(FlipbookClock { now });
}

/// The displayed action of one live battle bubble, and the presented tick at which it began.
///
/// Inserted with the bubble and mutated in place by `sync_live_battle_system` only (D-8). Free
/// loops (Idle, Walk, Cast) restart from their first layer at `since` (D-5); Attack and
/// Constructing take their timing from the record and ignore `since`.
// PERFORMANCE RATIONALE:
// Why: Recomputing a loop's start from the record each frame is impossible (the record carries
// no start), and inserting a component per change would queue a command and an archetype move.
// What: The phase is spawned with the bubble and overwritten in place with `set_if_neq`, so a
// frame on which no action changes writes nothing and flags nothing as changed.
// Trade-off: One more component on every live bubble, eight bytes of state the snapshot lacks.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlipbookPhase {
    /// What the bubble shows ([`displayed_action`]).
    pub action: AnimState,
    /// Presented simulation tick at which `action` began to show.
    pub since: Tick,
}

impl FlipbookPhase {
    /// A phase that begins `action` at the presented tick `tick`.
    #[must_use]
    pub const fn new(action: AnimState, tick: Tick) -> Self {
        Self {
            action,
            since: tick,
        }
    }

    /// The phase after the bubble shows `action` at the presented tick `tick`: unchanged while
    /// the action lasts, restarted at `tick` when it changes.
    #[must_use]
    pub fn observed(self, action: AnimState, tick: Tick) -> Self {
        if self.action == action {
            self
        } else {
            Self::new(action, tick)
        }
    }
}

/// What a battle bubble shows for its presented `record` (D-5).
///
/// `Attack` while the record publishes `Attack`, and also while it publishes `Idle` inside an
/// attack cycle (the cooldown, and the first tick of a swing), provided the atlas has an
/// `Attack` slice. Otherwise the published action.
#[must_use]
pub fn displayed_action(
    meta: Option<&FlipbookAtlasMeta>,
    record: &BubbleRenderInstance,
) -> AnimState {
    let has_attack = meta.is_some_and(|meta| meta.actions.contains_key(&AnimState::Attack));
    match record.anim_state {
        AnimState::Idle if has_attack && record.attack_swing.is_some() => AnimState::Attack,
        published => published,
    }
}

/// Clock units of one pass over `slice` of `meta`: `frame_count / fps` seconds of game time.
///
/// The manifest carries no per-slice duration yet (plan WP3 adds `duration_ms`), so the atlas
/// rate is used, as D-6 says for legacy atlases. A validated manifest has `fps >= 1`; an `fps`
/// of zero yields period 0, which [`FlipbookSpan::new`] refuses. A period beyond `u32`
/// saturates and is then refused by the span for any slice of more than one frame.
#[must_use]
pub fn slice_period(meta: &FlipbookAtlasMeta, slice: &ActionSlice) -> u32 {
    if meta.fps == 0 {
        return 0;
    }
    let units = u64::from(slice.frame_count) * CLOCK_UNITS_PER_SECOND / u64::from(meta.fps);
    u32::try_from(units).unwrap_or(u32::MAX)
}

/// The flipbook lanes of a battle bubble showing `record` at the presented tick `presented`,
/// with its displayed action and start in `phase` (D-5).
///
/// | `phase.action` | lanes |
/// |---|---|
/// | `Attack` with a swing | a loop over the swing's period, aligned so the slice's first layer shows at the release tick |
/// | `Constructing` | the layer `start + min(⌊progress · count⌋, count − 1)` of the build progress |
/// | any other | the slice at its clip speed from `phase.since`: a loop, or once as the manifest says |
/// | no atlas or no slice | layer 0 |
///
/// # Errors
/// [`FlipbookLanesError`] when the slice cannot be played by the integer clock (no frames, no
/// period, or a period too long for its frame count).
pub fn battle_lanes(
    meta: Option<&FlipbookAtlasMeta>,
    record: &BubbleRenderInstance,
    presented: Tick,
    phase: FlipbookPhase,
) -> Result<FlipbookLanes, FlipbookLanesError> {
    let Some((meta, slice)) =
        meta.and_then(|meta| meta.actions.get(&phase.action).map(|slice| (meta, slice)))
    else {
        return Ok(FlipbookLanes::fixed(0));
    };
    match (phase.action, record.attack_swing) {
        (AnimState::Attack, Some(swing)) => {
            let period = u32::from(swing.period_ticks).wrapping_mul(CLOCK_UNITS_PER_TICK);
            let span = FlipbookSpan::new(slice.start_frame, slice.frame_count, period)?;
            // The release tick R = presented − elapsed + damage point. Sustained fire at one
            // period moves R by whole periods, so the canonical lanes stay equal from swing to
            // swing.
            let release = i64::from(presented.0) - i64::from(swing.elapsed_ticks)
                + i64::from(swing.damage_point_ticks);
            Ok(FlipbookLanes::looping(
                span,
                clock_at_tick(release),
                slice.looping,
            ))
        }
        (AnimState::Constructing, _) => {
            let span = FlipbookSpan::new(slice.start_frame, slice.frame_count, 1)?;
            Ok(FlipbookLanes::fixed(
                span.base() + progress_step(record.build_or_queue_progress, span.count()),
            ))
        }
        _ => {
            let span = FlipbookSpan::new(
                slice.start_frame,
                slice.frame_count,
                slice_period(meta, slice),
            )?;
            let since = clock_at_tick(i64::from(phase.since.0));
            Ok(if slice.looping {
                FlipbookLanes::looping(span, since, true)
            } else {
                FlipbookLanes::once(span, since, slice.hold_last_frame)
            })
        }
    }
}

/// The clock time at the start of tick `tick`, wrapping like the clock (negative ticks wrap
/// backwards).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the clock keeps the low 32 bits of the tick time by design"
)]
const fn clock_at_tick(tick: i64) -> u32 {
    tick.wrapping_mul(CLOCK_UNITS_PER_TICK as i64) as u32
}

/// `min(⌊progress · count⌋, count − 1)` for a published build progress, exact for the
/// published steps of 1/65536. No progress counts as finished, as the shell's `build_progress`
/// lane does.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "65536 is exact in f32; the progress is clamped to [0, 1] and scaled to at most 65536"
)]
fn progress_step(progress: Option<f32>, count: u32) -> u32 {
    let steps = (progress.unwrap_or(1.0).clamp(0.0, 1.0) * PROGRESS_STEPS as f32).round() as u64;
    let step = steps * u64::from(count) / u64::from(PROGRESS_STEPS);
    u32::try_from(step).map_or(count - 1, |step| step.min(count - 1))
}

/// Steps of the published build progress: the snapshot publishes multiples of 1/65536.
const PROGRESS_STEPS: u32 = 65_536;

/// The material uniform of a live battle bubble: the record packed with its battle lanes, the
/// displayed action in `anim_state`, and `is_attacking` held through the whole attack cycle.
///
/// The shader reads `anim_state` only to tell a foundation, which the displayed action keeps, and
/// `is_attacking` for the target-lock ring. Both follow the displayed action rather than the
/// weapon's sub-phases, so steady fire modifies no material (D-5). Selection is applied by the
/// caller.
///
/// # Errors
/// As [`battle_lanes`].
// PERFORMANCE RATIONALE:
// Why: A material whose uniform changes is prepared again by Bevy (a new uniform buffer and bind
// group). The published `anim_state` and `is_attacking` flip between a swing's windup or
// backswing and its cooldown, twice per swing for every unit in a fight.
// What: The uniform carries the displayed action and "in an attack cycle" instead of the weapon's
// sub-phase, and the Attack lanes are canonical (origin 0, offset of the release mod period).
// Trade-off: The target-lock ring stays on through the cooldown (a visual choice under Q0020),
// in exchange for no material preparation at all during sustained fire at one period.
pub fn battle_uniform(
    meta: Option<&FlipbookAtlasMeta>,
    record: &BubbleRenderInstance,
    style: PlayerStyle,
    presented: Tick,
    phase: FlipbookPhase,
) -> Result<BubbleUniformGpu, FlipbookLanesError> {
    let lanes = battle_lanes(meta, record, presented, phase)?;
    let mut uniform = BubbleUniformGpu::from_instance(record, style, lanes);
    uniform.anim_state = phase.action as u32;
    uniform.is_attacking = u32::from(record.is_attacking || record.attack_swing.is_some());
    Ok(uniform)
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

    // --- The battle's display rules (WP5) --------------------------------------------------------

    use asset_baker::manifest::FlipbookLayout;
    use sim_types::{AttackSwing, EntityId, PlayerId, ShapeKind, UnitTypeId};
    use std::collections::BTreeMap;

    /// An atlas at 15 frames per second: Idle 0..10 looping, Walk 10..20 looping, Attack 20..30
    /// played once, Constructing 30..40 held, as the shipped unit manifests are laid out.
    fn atlas() -> FlipbookAtlasMeta {
        let slice = |start_frame, looping, hold_last_frame| ActionSlice {
            start_frame,
            frame_count: 10,
            looping,
            hold_last_frame,
        };
        FlipbookAtlasMeta {
            name: "fixture".to_owned(),
            frame_width: 8,
            frame_height: 8,
            fps: 15,
            total_frames: 40,
            layout: FlipbookLayout::VerticalStrip,
            actions: BTreeMap::from([
                (AnimState::Idle, slice(0, true, false)),
                (AnimState::Walk, slice(10, true, false)),
                (AnimState::Attack, slice(20, false, false)),
                (AnimState::Constructing, slice(30, false, true)),
            ]),
        }
    }

    fn record(anim_state: AnimState, attack_swing: Option<AttackSwing>) -> BubbleRenderInstance {
        BubbleRenderInstance {
            entity_id: EntityId(7),
            shape: ShapeKind::UnitSphere,
            silhouette: sim_types::SilhouetteClass::LightProngs,
            unit_type: UnitTypeId(1),
            owner: PlayerId(0),
            world_pos: [0.0; 3],
            ground_z: 0.0,
            radius: 0.5,
            velocity: [0.0; 2],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: sim_types::WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state,
            attack_swing,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            cloaked: false,
            speed_boosted: false,
        }
    }

    fn swing(elapsed_ticks: u16) -> AttackSwing {
        AttackSwing {
            elapsed_ticks,
            damage_point_ticks: 2,
            period_ticks: 15,
        }
    }

    /// Lanes of `record` presented at `tick`, displaying what the rules say since `since`.
    fn lanes_at(record: &BubbleRenderInstance, tick: u32, since: u32) -> FlipbookLanes {
        let meta = atlas();
        let phase = FlipbookPhase::new(displayed_action(Some(&meta), record), Tick(since));
        battle_lanes(Some(&meta), record, Tick(tick), phase)
            .expect("invariant: the fixture's slices are playable")
    }

    #[test]
    fn the_presented_clock_is_ticks_and_the_floored_fraction() {
        assert_eq!(presented_clock(Tick(0), 0.0), 0);
        assert_eq!(presented_clock(Tick(3), 0.0), 3 * 1024);
        assert_eq!(presented_clock(Tick(3), 0.5), 3 * 1024 + 512);
        assert_eq!(presented_clock(Tick(3), 0.999_999_9), 3 * 1024 + 1023);
        assert_eq!(presented_clock(Tick(3), 1.0), 3 * 1024 + 1023, "clamped");
        assert_eq!(presented_clock(Tick(3), -1.0), 3 * 1024, "clamped");
        // The clock wraps with the tick count, as the shader's u32 does.
        assert_eq!(presented_clock(Tick(u32::MAX), 0.0), u32::MAX - 1023);
        assert_eq!(presented_clock(Tick(1 << 22), 0.0), 0);
    }

    #[test]
    fn the_elapsed_clock_counts_25_ticks_a_second() {
        assert_eq!(elapsed_clock(Duration::ZERO), 0);
        assert_eq!(elapsed_clock(Duration::from_millis(40)), 1024);
        assert_eq!(elapsed_clock(Duration::from_secs(1)), 25 * 1024);
        // 2^32 units are 167 772.16 s: the clock wraps there.
        assert_eq!(elapsed_clock(Duration::from_secs(167_773)), 21_504);
    }

    #[test]
    fn the_cooldown_and_the_first_tick_of_a_swing_display_attack() {
        let meta = atlas();
        let shown = |anim, swing| displayed_action(Some(&meta), &record(anim, swing));
        assert_eq!(shown(AnimState::Attack, Some(swing(1))), AnimState::Attack);
        assert_eq!(shown(AnimState::Idle, Some(swing(0))), AnimState::Attack);
        assert_eq!(shown(AnimState::Idle, Some(swing(9))), AnimState::Attack);
        assert_eq!(shown(AnimState::Idle, None), AnimState::Idle);
        // A stutter step walks, even in cooldown.
        assert_eq!(shown(AnimState::Walk, Some(swing(9))), AnimState::Walk);
        assert_eq!(
            shown(AnimState::Constructing, None),
            AnimState::Constructing
        );
        // Without an Attack slice the cooldown stays Idle.
        let mut no_attack = atlas();
        no_attack.actions.remove(&AnimState::Attack);
        assert_eq!(
            displayed_action(Some(&no_attack), &record(AnimState::Idle, Some(swing(9)))),
            AnimState::Idle
        );
        assert_eq!(
            displayed_action(None, &record(AnimState::Idle, Some(swing(9)))),
            AnimState::Idle
        );
    }

    #[test]
    fn the_release_tick_shows_the_attack_slice_first_layer() {
        // A swing that began at tick 100 releases at 102 and lasts 15 ticks.
        let mut lanes = None;
        for elapsed in 0..15u16 {
            let tick = 100 + u32::from(elapsed);
            let anim = if elapsed <= 6 {
                AnimState::Attack
            } else {
                AnimState::Idle
            };
            let at = lanes_at(&record(anim, Some(swing(elapsed))), tick, 0);
            assert_eq!(*lanes.get_or_insert(at), at, "constant through the cycle");
            let layer = flipbook_layer(tick * 1024, &at);
            match elapsed {
                2 => assert_eq!(layer, 20, "release"),
                1 => assert_eq!(layer, 29, "the tick before shows the last layer"),
                _ => assert!((20..30).contains(&layer)),
            }
        }
        let lanes = lanes.expect("invariant: the loop above ran");
        assert_eq!(lanes.period, 15 * 1024);
        assert_eq!(
            lanes.mode,
            FlipbookMode::Loop as u32,
            "the slice is not a loop: no blend"
        );
        // The next swing at the same period gives the same lanes: no material changes.
        let next = lanes_at(&record(AnimState::Idle, Some(swing(0))), 115, 0);
        assert_eq!(next, lanes);
        // A swing at another phase moves the offset so its own release shows layer 20.
        let late = lanes_at(&record(AnimState::Attack, Some(swing(2))), 119, 0);
        assert_eq!(flipbook_layer(119 * 1024, &late), 20);
        assert_ne!(late, lanes);
    }

    #[test]
    fn free_loops_start_at_their_first_layer_and_play_at_the_atlas_rate() {
        let walk = record(AnimState::Walk, None);
        let lanes = lanes_at(&walk, 500, 480);
        // 10 frames at 15 frames per second: 2/3 s, 50/3 ticks.
        assert_eq!(lanes.period, 10 * 25 * 1024 / 15);
        assert_eq!(lanes.mode, FlipbookMode::Loop as u32 | ANIM_BLEND_BIT);
        assert_eq!(
            flipbook_layer(480 * 1024, &lanes),
            10,
            "first layer at since"
        );
        assert_eq!(flipbook_layer(481 * 1024, &lanes), 10);
        assert_eq!(flipbook_layer(482 * 1024, &lanes), 11);
        // The lanes do not depend on the presented tick, only on since.
        assert_eq!(lanes_at(&walk, 900, 480), lanes);
        // A one-shot slice plays once from since.
        let mut once = atlas();
        if let Some(walk) = once.actions.get_mut(&AnimState::Walk) {
            walk.looping = false;
            walk.hold_last_frame = true;
        }
        let phase = FlipbookPhase::new(AnimState::Walk, Tick(480));
        let lanes =
            battle_lanes(Some(&once), &walk, Tick(500), phase).expect("invariant: playable");
        assert_eq!(
            lanes,
            FlipbookLanes::once(
                FlipbookSpan::new(10, 10, 10 * 25 * 1024 / 15).expect("invariant: valid"),
                480 * 1024,
                true,
            )
        );
    }

    #[test]
    fn a_foundation_shows_the_layer_of_its_build_progress() {
        let mut foundation = record(AnimState::Constructing, None);
        for (progress, layer) in [
            (0.0, 30),
            (0.099_99, 30),
            (0.1, 31),
            (0.5, 35),
            (0.999_984_74, 39),
            (1.0, 39),
        ] {
            foundation.build_or_queue_progress = Some(progress);
            assert_eq!(
                lanes_at(&foundation, 10, 0),
                FlipbookLanes::fixed(layer),
                "progress {progress}"
            );
        }
        // Every published step k/65536 lands on ⌊k · 10 / 65536⌋ exactly.
        for k in [6553_u16, 6554, 13_107, 65_535] {
            foundation.build_or_queue_progress = Some(f32::from(k) / 65_536.0);
            assert_eq!(
                lanes_at(&foundation, 10, 0).base,
                30 + u32::from(k) * 10 / 65_536,
                "step {k}"
            );
        }
    }

    #[test]
    fn an_action_without_a_slice_shows_layer_zero() {
        let cast = record(AnimState::Cast, None);
        assert_eq!(lanes_at(&cast, 10, 0), FlipbookLanes::fixed(0));
        let phase = FlipbookPhase::new(AnimState::Idle, Tick(0));
        assert_eq!(
            battle_lanes(None, &record(AnimState::Idle, None), Tick(5), phase),
            Ok(FlipbookLanes::fixed(0))
        );
    }

    #[test]
    fn an_unplayable_slice_is_a_typed_error() {
        let mut meta = atlas();
        meta.fps = 0;
        let idle = record(AnimState::Idle, None);
        let phase = FlipbookPhase::new(AnimState::Idle, Tick(0));
        assert_eq!(
            battle_lanes(Some(&meta), &idle, Tick(0), phase),
            Err(FlipbookLanesError::ZeroPeriod)
        );
        let mut meta = atlas();
        meta.fps = 1;
        if let Some(idle) = meta.actions.get_mut(&AnimState::Idle) {
            idle.frame_count = 2000;
        }
        assert!(matches!(
            battle_lanes(Some(&meta), &idle, Tick(0), phase),
            Err(FlipbookLanesError::TooLong { count: 2000, .. })
        ));
        let attack = record(
            AnimState::Attack,
            Some(AttackSwing {
                elapsed_ticks: 0,
                damage_point_ticks: 0,
                period_ticks: 0,
            }),
        );
        let phase = FlipbookPhase::new(AnimState::Attack, Tick(0));
        assert_eq!(
            battle_lanes(Some(&atlas()), &attack, Tick(0), phase),
            Err(FlipbookLanesError::ZeroPeriod)
        );
    }

    #[test]
    fn a_phase_restarts_only_when_the_action_changes() {
        let phase = FlipbookPhase::new(AnimState::Idle, Tick(4));
        assert_eq!(phase.observed(AnimState::Idle, Tick(90)), phase);
        assert_eq!(
            phase.observed(AnimState::Walk, Tick(90)),
            FlipbookPhase::new(AnimState::Walk, Tick(90))
        );
    }

    #[test]
    fn the_battle_uniform_holds_the_attack_through_the_cycle() {
        let meta = atlas();
        let style = PlayerStyle {
            srgb: [1.0; 3],
            linear: Vec4::ONE,
            ui: bevy_egui::egui::Color32::WHITE,
        };
        let pack = |anim, elapsed: u16, is_attacking, tick: u32| {
            let mut bubble = record(anim, Some(swing(elapsed)));
            bubble.is_attacking = is_attacking;
            let phase = FlipbookPhase::new(displayed_action(Some(&meta), &bubble), Tick(0));
            battle_uniform(Some(&meta), &bubble, style, Tick(tick), phase)
                .expect("invariant: playable")
        };
        // Windup, then cooldown: the weapon's sub-phases change no lane the shader reads.
        let windup = pack(AnimState::Attack, 1, true, 101);
        let cooldown = pack(AnimState::Idle, 9, false, 109);
        assert!(windup.renders_same_as(&cooldown));
        assert_eq!(windup.anim_state, AnimState::Attack as u32);
        assert_eq!(windup.is_attacking, 1);
    }
}
