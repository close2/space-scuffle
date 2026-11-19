//! The retained table of lasting decals (plan `2026-11-19-retained-effect-instances` D-2, D-5).
//!
//! # Responsibility
//! Owns [`DecalTable`]: one row per placed decal of a live effect, holding a [`DecalRecord`]
//! (everything fixed by its effect's state) and an order key, kept in the order the frame
//! build used to emit decals: the `Critical` pass over slots ascending, then the `Detail` pass,
//! each by layer, placement and instance. Also owns the record writer ([`write_rows`]), which
//! runs the per-instance work `push_decal` ran (placement, clip, hashes, the yaw, the surface)
//! once when an effect's decal inputs change, and [`age_decal`], the pure function of a record,
//! the time and the camera that gives the decal's [`DecalInstance`] of a frame: the reference
//! that `assets/shaders/effects/decal_age.wgsl` ports (D-2, D-10), and the table's GPU form
//! ([`GpuDecalRecord`], [`GpuCurveKey`]) the render world copies the rows into.
//!
//! # How it fits
//! [`super::runtime::EffectRuntime`] marks an effect dirty whenever an API changes one of its
//! decal inputs (spawn, hold, a merged line, a release, a move, a fog clip, a retirement of its
//! detail or marks, its end, a change of `only`) and when one of its decal layers ends; once a
//! frame [`DecalTable::rewrite`] frees each dirty effect's rows and writes them anew. Each
//! rewrite lists the rows it wrote ([`DecalTable::patches`]) under a new
//! [`DecalTable::generation`]; the render world copies those rows (or every live row after a
//! gap in the generations, [`DecalTable::copy_since`]) into its retained row buffer, and a
//! compute pass ages the rows in [`DecalTable::order`] into the decal buffer the unchanged
//! `decal.wgsl` draws (`super::render`). So the picture is the same pure function of the event
//! stream and `Time<Virtual>` as before, and the main world's per-frame work for a decal is
//! nothing unless its effect changed. Tests and tools age the rows on the CPU with the same
//! reference ([`DecalTable::evaluate`], [`DecalTable::age_rows`]).
//!
//! # Invariants
//! - Every vector is allocated at construction, at the decal budget's or the pool's capacity,
//!   and only cleared and refilled: a steady frame allocates nothing.
//! - The rows of an effect are exactly the placed, seen instances of its decal layers that are
//!   drawn by its `only` filter and have not ended, while it holds decal slots.
//! - Keys are unique, so the order is a total order; it is re-sorted only on frames whose rows
//!   changed.
//! - Time runs forward (`Time<Virtual>` never goes back; the lab resets the runtime between
//!   takes): a layer dropped at a rewrite because it ended is never brought back by a later,
//!   earlier `now`. The aging itself is a pure function of `now` (a frozen or jumping `now`
//!   draws what the reference draws for it); only the rows' lifetimes assume forward time, so
//!   a backward step without a reset (no client path makes one, plan D-8) could leave a layer
//!   that ended undrawn.
//! - An effect never has more rows than the decal slots it was granted at birth: a placed
//!   decal past its grant (a held line lengthened under a line-placed decal layer; the shipped
//!   vocabulary has none) gets no row and is counted in [`DecalTable::failed`], which the
//!   runtime adds to its critical overflow, never dropped silently. As the ledger never grants
//!   more slots than the decal budget holds, the live rows fit the budget's capacity, and so
//!   does the GPU's decal buffer, which has one decal per row of the order.
//! - Every time is aged through [`split_time`] and [`split_seconds`] (D-8), in Rust and in
//!   WGSL alike, so the GPU, which has no `f64`, computes the same ages as the reference.

use super::binding::{resolve_value, BoundValues};
use super::budget::{lod_skips, retire_fade, RETIRE_SECS};
use super::catalog::schema::{DecalYaw, Priority};
use super::catalog::{Block, CurveId, EffectCatalog, ElementRef, Layer, Pool};
use super::curve::easing_code;
use super::instances::DecalInstance;
use super::ledger::{mark_fade, MARK_RETIRE_SECS};
use super::placement::{hash3, instance_count, place, placed_seen, unit_float};
use super::pool::{EffectInstance, EffectInstancePool};
use super::runtime::{
    clip_window, flipbook_frame, layer_ended, layer_size_strength, layer_strength, placed_seed,
    variant_frame, with_fog, CameraView, Surface,
};
use bevy::math::Vec3;
use bytemuck::{Pod, Zeroable};

/// No row.
const NONE: u32 = u32::MAX;

/// Seconds ahead of a row layer's computed end at which its effect's rows are rewritten.
const EXPIRY_MARGIN: f32 = 1e-3;

/// Everything about one decal that its effect's state fixes, and the inputs of its aging.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecalRecord {
    /// The decal with every field its effect's state fixes set: centre, yaw, grazing, ramp
    /// row, clip, liquid, outline, darkening and surface; for a variant set its frame too.
    pub template: DecalInstance,
    /// The pool slot of its effect.
    pub slot: u32,
    /// Its effect's birth, seconds of `Time<Virtual>`.
    pub birth: f64,
    /// Its layer's start, seconds.
    pub start: f32,
    /// Its placement's delay, seconds.
    pub delay: f32,
    /// Its layer's span, seconds.
    pub span: f32,
    /// Whether its layer is held (a held layer of a held effect).
    pub held: bool,
    /// When its held effect was released, if it was.
    pub released_at: Option<f64>,
    /// Its layer's exit, seconds.
    pub exit: f32,
    /// Whether its layer is `Detail` (its effect's detail retirement fades it).
    pub detail: bool,
    /// When its effect's detail was retired, if it was.
    pub detail_retired_at: Option<f64>,
    /// When its effect's lasting marks were retired, if they were.
    pub marks_retired_at: Option<f64>,
    /// Its layer's intensity curve.
    pub curve: Option<CurveId>,
    /// Its layer's size curve.
    pub size_curve: Option<CurveId>,
    /// Its look's erosion curve.
    pub erosion_curve: Option<CurveId>,
    /// Its layer's ramp curve.
    pub ramp_curve: Option<CurveId>,
    /// Its layer's bound strength factor on the intensity.
    pub strength: f32,
    /// Its layer's resolved size, world units.
    pub size: f32,
    /// Its layer's bound strength factor on the size.
    pub size_strength: f32,
    /// Its own size jitter factor.
    pub jitter: f32,
    /// Its look's box depth, world units.
    pub depth: f32,
    /// Its look's width over its length.
    pub width: f32,
    /// The flipbook fitted to its life; `None` for a variant set (its frame is in the template).
    pub flipbook: Option<ElementRef>,
}

/// A decal's clock at a time: its life, its exit, its erosion floor and its fade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecalClock {
    /// Share of its layer's span gone, in [0, 1].
    pub life: f32,
    /// Share of a released held layer's exit gone (its own clock).
    pub exit: f32,
    /// The exit its layer's erosion is at least (the layer's clock without a delay).
    pub exit_floor: f32,
    /// Its fade from detail and mark retirement.
    pub fade: f32,
}

/// Seconds of `Time<Virtual>` from `since` to `now`, as `f32`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "seconds since an event fit f32"
)]
fn seconds(now: f64, since: f64) -> f32 {
    (now - since) as f32
}

/// A time of `Time<Virtual>` (seconds, `f64`) as two `f32`: its nearest `f32` and the rest
/// (D-8). WGSL has no `f64`, so the GPU receives every time in this form.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the split is the point: hi is the nearest f32, lo the rounding rest"
)]
pub fn split_time(time: f64) -> [f32; 2] {
    let hi = time as f32;
    let lo = (time - f64::from(hi)) as f32;
    [hi, lo]
}

/// Seconds from `since` to `now`, both split by [`split_time`]: the difference of the high
/// parts, exact by Sterbenz's lemma while `since` is within a factor of two of `now`, plus the
/// difference of the small rests (D-8). The expression `decal_age.wgsl` evaluates.
#[must_use]
pub fn split_seconds(now: [f32; 2], since: [f32; 2]) -> f32 {
    (now[0] - since[0]) + (now[1] - since[1])
}

/// Seconds from `since` to `now` through the split form.
fn aged_seconds(now: [f32; 2], since: f64) -> f32 {
    split_seconds(now, split_time(since))
}

/// The clock of `record` at `now`, or `None` when it draws nothing: its effect's detail faded
/// out, not started, past its span, or eroded out after a release. The same expressions, in
/// the same order, as `write_layers`, `marks_fade` and `layer_clock`.
#[must_use]
pub fn decal_clock(record: &DecalRecord, now: f64) -> Option<DecalClock> {
    let now = split_time(now);
    let detail_fade = match (record.detail, record.detail_retired_at) {
        (true, Some(retired)) => retire_fade(aged_seconds(now, retired)),
        _ => 1.0,
    };
    if detail_fade <= 0.0 {
        return None;
    }
    let fade = record.marks_retired_at.map_or(detail_fade, |retired| {
        detail_fade * mark_fade(aged_seconds(now, retired))
    });
    let since_birth = aged_seconds(now, record.birth) - record.start;
    let released_exit = || match record.released_at {
        Some(released) => (aged_seconds(now, released) / record.exit.max(1e-3)).max(0.0),
        None => 0.0,
    };
    let exit_floor = if since_birth - 0.0 < 0.0 || !record.held {
        0.0
    } else {
        let exit = released_exit();
        if exit >= 1.0 {
            0.0
        } else {
            exit
        }
    };
    let age = since_birth - record.delay;
    if age < 0.0 {
        return None;
    }
    let exit = if record.held {
        let exit = released_exit();
        if exit >= 1.0 {
            return None;
        }
        exit
    } else {
        if age > record.span {
            return None;
        }
        0.0
    };
    Some(DecalClock {
        life: (age / record.span).clamp(0.0, 1.0),
        exit,
        exit_floor,
        fade,
    })
}

/// The decal of `record` at `clock`, or `None` when it is smaller than `min_screen_px` from
/// `camera`: the same expressions, in the same order, as `write_instance` and `push_decal`.
#[must_use]
pub fn decal_at(
    record: &DecalRecord,
    clock: DecalClock,
    catalog: &EffectCatalog,
    camera: Option<&CameraView>,
) -> Option<DecalInstance> {
    let DecalClock {
        life,
        exit,
        exit_floor,
        fade,
    } = clock;
    let intensity =
        catalog.sample(record.curve, life, 1.0) * fade * (1.0 - exit * exit) * record.strength;
    let extent = record.size
        * catalog.sample(record.size_curve, life, 1.0)
        * record.size_strength
        * record.jitter;
    let position = Vec3::from_array(record.template.center);
    if let Some(camera) = camera {
        if lod_skips(
            camera.pixels(extent * 2.0, position),
            catalog.budget.min_screen_px,
        ) {
            return None;
        }
    }
    let (layer_a, layer_b, frame_mix) = record.flipbook.map_or(
        (
            record.template.layer_a,
            record.template.layer_b,
            record.template.frame_mix,
        ),
        |element| flipbook_frame(&element, None, 0.0, life, true),
    );
    Some(DecalInstance {
        half_extents: [extent, record.depth, extent * record.width],
        layer_a,
        layer_b,
        frame_mix,
        intensity,
        erosion: catalog
            .sample(record.erosion_curve, life, 0.0)
            .max(exit_floor),
        ramp_scale: catalog.sample(record.ramp_curve, life, 1.0),
        ..record.template
    })
}

/// The decal `record` draws at `now` from `camera`, if any (D-2: the reference the GPU's
/// `decal_age.wgsl` ports).
#[must_use]
pub fn age_decal(
    record: &DecalRecord,
    now: f64,
    catalog: &EffectCatalog,
    camera: Option<&CameraView>,
) -> Option<DecalInstance> {
    decal_at(record, decal_clock(record, now)?, catalog, camera)
}

/// How near a row is to a boundary of its visibility at a time: where the GPU's arithmetic,
/// which may round differently from the reference by a few ulp, can flip it on or off (D-10
/// check 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgeMargins {
    /// Seconds to the nearest start or end of its drawing: its start, the end of its span, the
    /// end of a released held layer's exit, of its detail fade or of its marks' fade.
    pub seconds: f32,
    /// Pixels between its projected size and the level-of-detail threshold; infinite without a
    /// camera or when it has no clock.
    pub pixels: f32,
}

/// [`AgeMargins`] of `record` at `now` from `camera` (exact `f64` ages, so the margins measure
/// the distance to the boundary rather than one rounding of it).
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "margins are reported in f32 seconds"
)]
pub fn age_margins(
    record: &DecalRecord,
    now: f64,
    catalog: &EffectCatalog,
    camera: Option<&CameraView>,
) -> AgeMargins {
    let since_birth = now - record.birth - f64::from(record.start);
    let age = since_birth - f64::from(record.delay);
    let mut seconds = age.abs();
    if !record.held {
        seconds = seconds.min((age - f64::from(record.span)).abs());
    }
    if let Some(released) = record.released_at {
        let exit = f64::from(record.exit.max(1e-3));
        seconds = seconds.min((now - released - exit).abs());
    }
    if let (true, Some(retired)) = (record.detail, record.detail_retired_at) {
        seconds = seconds.min((now - retired - f64::from(RETIRE_SECS)).abs());
    }
    if let Some(retired) = record.marks_retired_at {
        seconds = seconds.min((now - retired - f64::from(MARK_RETIRE_SECS)).abs());
    }
    let pixels = match (camera, decal_clock(record, now)) {
        (Some(camera), Some(clock)) => {
            let extent = record.size
                * catalog.sample(record.size_curve, clock.life, 1.0)
                * record.size_strength
                * record.jitter;
            let position = Vec3::from_array(record.template.center);
            (camera.pixels(extent * 2.0, position) - catalog.budget.min_screen_px).abs()
        }
        _ => f32::INFINITY,
    };
    AgeMargins {
        seconds: seconds as f32,
        pixels,
    }
}

/// The decal the GPU writes for a row that draws nothing at a frame: every field zero, so its
/// box has no extent and rasterises nothing (D-5).
#[must_use]
pub fn hidden_decal() -> DecalInstance {
    DecalInstance::zeroed()
}

/// Bits of [`GpuDecalRecord::flags`].
pub mod record_flags {
    /// The row's layer is held.
    pub const HELD: u32 = 1;
    /// The row's layer is `Detail`.
    pub const DETAIL: u32 = 2;
    /// Its held effect was released ([`super::GpuDecalRecord::released`] is set).
    pub const RELEASED: u32 = 4;
    /// Its effect's detail was retired.
    pub const DETAIL_RETIRED: u32 = 8;
    /// Its effect's lasting marks were retired.
    pub const MARKS_RETIRED: u32 = 16;
    /// Its flipbook is fitted to its life (not a variant set).
    pub const FLIPBOOK: u32 = 32;
}

/// [`GpuDecalRecord::curves`] entry of a layer without that curve.
pub const NO_CURVE: u32 = u32::MAX;

/// A row of the table as the GPU reads it: `DecalRecord` in `decal_age.wgsl`'s layout, every
/// time split by [`split_time`] (D-8), every option a flag.
// PERFORMANCE RATIONALE:
// Why: The compute pass reads one record per live decal per frame from a storage buffer.
// What: A `Pod` record of sixteen 16-byte rows (the decal template, then 28 scalars and three
// pad words) matching the WGSL struct, so a row is copied as bytes.
// Trade-off: Options become flags and absent curves a sentinel, and the record is wider than
// its information (256 bytes) to keep WGSL's 16-byte struct alignment.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Pod, Zeroable)]
pub struct GpuDecalRecord {
    /// [`DecalRecord::template`].
    pub template: DecalInstance,
    /// Its effect's birth, split.
    pub birth: [f32; 2],
    /// When its held effect was released, split (zero unless [`record_flags::RELEASED`]).
    pub released: [f32; 2],
    /// When its effect's detail was retired, split.
    pub detail_retired: [f32; 2],
    /// When its effect's marks were retired, split.
    pub marks_retired: [f32; 2],
    /// Its layer's start, seconds.
    pub start: f32,
    /// Its placement's delay, seconds.
    pub delay: f32,
    /// Its layer's span, seconds.
    pub span: f32,
    /// Its layer's exit, seconds.
    pub exit: f32,
    /// Its layer's bound strength factor on the intensity.
    pub strength: f32,
    /// Its layer's resolved size, world units.
    pub size: f32,
    /// Its layer's bound strength factor on the size.
    pub size_strength: f32,
    /// Its own size jitter factor.
    pub jitter: f32,
    /// Its look's box depth, world units.
    pub depth: f32,
    /// Its look's width over its length.
    pub width: f32,
    /// The first array layer of its fitted flipbook.
    pub flipbook_first: u32,
    /// The frames of its fitted flipbook.
    pub flipbook_frames: u32,
    /// Its intensity, size, erosion and ramp curves, or [`NO_CURVE`].
    pub curves: [u32; 4],
    /// [`record_flags`] bits.
    pub flags: u32,
    /// Padding to a whole 16-byte row.
    pub pad: [u32; 3],
}

impl GpuDecalRecord {
    /// The GPU form of `record`.
    #[must_use]
    pub fn of(record: &DecalRecord) -> Self {
        let time = |at: Option<f64>| at.map_or([0.0, 0.0], split_time);
        let flag = |set: bool, bit: u32| if set { bit } else { 0 };
        let curve = |id: Option<CurveId>| id.map_or(NO_CURVE, |id| u32::from(id.0));
        let flipbook = record.flipbook.unwrap_or(ElementRef {
            first_layer: 0,
            frames: 1,
            duration_secs: None,
        });
        Self {
            template: record.template,
            birth: split_time(record.birth),
            released: time(record.released_at),
            detail_retired: time(record.detail_retired_at),
            marks_retired: time(record.marks_retired_at),
            start: record.start,
            delay: record.delay,
            span: record.span,
            exit: record.exit,
            strength: record.strength,
            size: record.size,
            size_strength: record.size_strength,
            jitter: record.jitter,
            depth: record.depth,
            width: record.width,
            flipbook_first: flipbook.first_layer,
            flipbook_frames: flipbook.frames,
            curves: [
                curve(record.curve),
                curve(record.size_curve),
                curve(record.erosion_curve),
                curve(record.ramp_curve),
            ],
            flags: flag(record.held, record_flags::HELD)
                | flag(record.detail, record_flags::DETAIL)
                | flag(record.released_at.is_some(), record_flags::RELEASED)
                | flag(
                    record.detail_retired_at.is_some(),
                    record_flags::DETAIL_RETIRED,
                )
                | flag(
                    record.marks_retired_at.is_some(),
                    record_flags::MARKS_RETIRED,
                )
                | flag(record.flipbook.is_some(), record_flags::FLIPBOOK),
            pad: [0; 3],
        }
    }
}

/// One key of the GPU curve table: `decal_age.wgsl`'s `CurveKey`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, Pod, Zeroable)]
pub struct GpuCurveKey {
    /// The key's normalised time.
    pub time: f32,
    /// Its value.
    pub value: f32,
    /// [`easing_code`] of its easing to the next key.
    pub easing: u32,
    /// Padding to 16 bytes.
    pub pad: u32,
}

/// The GPU curve table of `catalog`: per curve, in [`CurveId`] order, its first key and key
/// count, and every curve's keys after one another. Written once per vocabulary.
#[must_use]
pub fn curve_table(catalog: &EffectCatalog) -> (Vec<[u32; 2]>, Vec<GpuCurveKey>) {
    let mut ranges = Vec::with_capacity(catalog.curves.len());
    let mut keys = Vec::new();
    for curve in &catalog.curves {
        let first = u32::try_from(keys.len()).unwrap_or(u32::MAX);
        keys.extend(
            curve
                .keys()
                .iter()
                .map(|&(time, value, easing)| GpuCurveKey {
                    time,
                    value,
                    easing: easing_code(easing),
                    pad: 0,
                }),
        );
        let count = u32::try_from(curve.keys().len()).unwrap_or(u32::MAX);
        ranges.push([first, count]);
    }
    (ranges, keys)
}

/// What a copy of the table that last took generation `applied` must copy to equal it (the
/// render world's row buffer, D-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowCopy {
    /// It is current.
    Nothing,
    /// It took the previous generation: the last rewrite's [`DecalTable::patches`].
    Patches,
    /// It missed a generation, or never took one: every live row ([`DecalTable::order`]).
    Every,
}

/// The retained decal rows of the live effects.
// PERFORMANCE RATIONALE:
// Why: Half the frame build's instructions went to re-placing, re-clipping and re-hashing every
// lasting decal every frame, although those values change only when its effect changes (B-137).
// What: Rows allocated once at the decal budget, chained per pool slot with a free list, a
// dirty bitset and list of slots, and a per-slot expiry: only changed effects are rewritten.
// Trade-off: Every API that changes an effect must mark it (enforced by the pool being private
// and tested per API), and the rows hold about 280 bytes per decal.
#[derive(Debug, Clone, PartialEq)]
pub struct DecalTable {
    /// Row records, by row.
    records: Vec<DecalRecord>,
    /// Row keys, by row.
    keys: Vec<u64>,
    /// The next row of the same effect, by row; [`NONE`] at the end of a chain.
    next: Vec<u32>,
    /// The first row of each pool slot's chain.
    head: Vec<u32>,
    /// Free rows, used last-in first-out.
    free: Vec<u32>,
    /// Whether each row is live, by row.
    live_rows: Vec<bool>,
    /// Live rows by key.
    order: Vec<u32>,
    /// Whether rows were added or removed since `order` was sorted.
    order_stale: bool,
    /// Dirty slots, one bit each.
    dirty_bits: Vec<u64>,
    /// Dirty slots, in the order they were marked.
    dirty: Vec<u32>,
    /// Per slot: the earliest end of a row's layer that is not held, in the effect's age, and
    /// the shortest exit of a row's held layer after its release (the rows change then).
    expiry: Vec<(f32, f32)>,
    /// Rows written by the last rewrite, in the order written: what the render world copies.
    patches: Vec<u32>,
    /// Rewrites so far; a clear skips one, so the render world sees a gap and copies every row.
    generation: u64,
    /// Sorts of the order so far (and clears): the render world copies the order when it moved.
    order_generation: u64,
    /// Live rows whose decal darkens what lies under it.
    darkening: u32,
    /// Placed decals that got no row: the table was full, or their effect's grant was used up
    /// (a sizing defect, counted as a critical overflow).
    pub failed: u64,
}

impl DecalTable {
    /// An empty table of `rows` rows for a pool of `slots` slots.
    #[must_use]
    pub fn with_capacity(rows: u32, slots: u32) -> Self {
        let (rows, slots) = (rows as usize, slots as usize);
        Self {
            records: Vec::with_capacity(rows),
            keys: Vec::with_capacity(rows),
            next: Vec::with_capacity(rows),
            live_rows: Vec::with_capacity(rows),
            head: vec![NONE; slots],
            free: Vec::with_capacity(rows),
            order: Vec::with_capacity(rows),
            order_stale: false,
            dirty_bits: vec![0; slots.div_ceil(64)],
            dirty: Vec::with_capacity(slots),
            expiry: vec![(f32::INFINITY, f32::INFINITY); slots],
            patches: Vec::with_capacity(rows),
            generation: 0,
            order_generation: 0,
            darkening: 0,
            failed: 0,
        }
    }

    /// Marks the effect in `slot` dirty: its rows are rewritten at the next frame.
    pub fn mark(&mut self, slot: u32) {
        let (word, bit) = ((slot / 64) as usize, slot % 64);
        let Some(bits) = self.dirty_bits.get_mut(word) else {
            return;
        };
        if *bits & (1 << bit) == 0 {
            *bits |= 1 << bit;
            self.dirty.push(slot);
        }
    }

    /// Marks every slot dirty (a change of `only`, or a test's from-scratch rebuild).
    pub fn mark_all(&mut self) {
        for slot in 0..u32::try_from(self.head.len()).unwrap_or(u32::MAX) {
            self.mark(slot);
        }
    }

    /// Removes every row. The generation skips one, so the render world copies the whole
    /// (empty) table at its next extraction rather than patches.
    pub fn clear(&mut self) {
        self.patches.clear();
        self.generation += 1;
        self.order_generation += 1;
        self.darkening = 0;
        self.records.clear();
        self.keys.clear();
        self.next.clear();
        self.live_rows.clear();
        self.head.fill(NONE);
        self.free.clear();
        self.order.clear();
        self.order_stale = false;
        self.dirty_bits.fill(0);
        self.dirty.clear();
        self.expiry.fill((f32::INFINITY, f32::INFINITY));
    }

    /// Marks dirty every effect of `pool` one of whose rows' layers ended by `now`.
    pub fn expire(&mut self, pool: &EffectInstancePool, now: f64) {
        for slot in 0..self.head.len() {
            let (until_age, until_exit) = self.expiry[slot];
            if self.head[slot] == NONE {
                continue;
            }
            let Some(instance) = pool.slot(slot) else {
                continue;
            };
            // A margin ahead of the ends, which are one expression of an exact test: a rewrite
            // a frame early keeps the layer (and comes again), so the rows of an ended layer
            // are gone on the very frame it ends.
            let age_passed = seconds(now, instance.birth) + EXPIRY_MARGIN > until_age;
            let exit_passed = instance
                .released_at
                .is_some_and(|released| seconds(now, released) + EXPIRY_MARGIN >= until_exit);
            if age_passed || exit_passed {
                self.mark(u32::try_from(slot).unwrap_or(u32::MAX));
            }
        }
    }

    /// Rewrites the rows of every dirty effect of `pool` from its state at `now`, under the
    /// priority filter `only`.
    pub fn rewrite(
        &mut self,
        pool: &EffectInstancePool,
        catalog: &EffectCatalog,
        now: f64,
        only: Option<Priority>,
    ) {
        self.patches.clear();
        self.generation += 1;
        for index in 0..self.dirty.len() {
            let slot = self.dirty[index];
            self.free_chain(slot);
            if let Some(instance) = pool.slot(slot as usize) {
                write_rows(self, slot, instance, catalog, (now, only));
            }
        }
        for slot in self.dirty.drain(..) {
            if let Some(bits) = self.dirty_bits.get_mut((slot / 64) as usize) {
                *bits &= !(1 << (slot % 64));
            }
        }
        if self.order_stale {
            // PERFORMANCE RATIONALE:
            // Why: The decals must be written in the order the frame build emitted them, which
            // the keys give; rows are added and freed in any order as effects change.
            // What: On a frame whose rows changed, the order is rebuilt from the live rows and
            // sorted by key (unique keys, so `sort_unstable` is a total order); on any other
            // frame it is kept as it is.
            // Trade-off: An O(n log n) sort of a few thousand row indices on busy frames, in
            // exchange for no ordered insertion structure to keep in step with the rows.
            let (keys, live) = (&self.keys, &self.live_rows);
            self.order.clear();
            self.order.extend(
                (0..u32::try_from(live.len()).unwrap_or(u32::MAX))
                    .filter(|row| live[*row as usize]),
            );
            self.order.sort_unstable_by_key(|row| keys[*row as usize]);
            self.order_stale = false;
            self.order_generation += 1;
        }
    }

    /// Frees the rows of `slot`.
    fn free_chain(&mut self, slot: u32) {
        let Some(head) = self.head.get_mut(slot as usize) else {
            return;
        };
        let mut row = std::mem::replace(head, NONE);
        if row == NONE {
            return;
        }
        while row != NONE {
            self.free.push(row);
            self.live_rows[row as usize] = false;
            if self.records[row as usize].template.darken != 0.0 {
                self.darkening = self.darkening.saturating_sub(1);
            }
            row = self.next[row as usize];
        }
        self.order_stale = true;
        self.expiry[slot as usize] = (f32::INFINITY, f32::INFINITY);
    }

    /// Adds a row of `slot` with `key` and `record`; counts a failure when the table is full.
    fn push(&mut self, slot: u32, key: u64, record: &DecalRecord) {
        let row = if let Some(row) = self.free.pop() {
            let index = row as usize;
            self.records[index] = *record;
            self.keys[index] = key;
            self.live_rows[index] = true;
            row
        } else if self.records.len() < self.records.capacity() {
            self.records.push(*record);
            self.keys.push(key);
            self.next.push(NONE);
            self.live_rows.push(true);
            u32::try_from(self.records.len() - 1).unwrap_or(NONE)
        } else {
            self.failed += 1;
            return;
        };
        let head = &mut self.head[slot as usize];
        self.next[row as usize] = *head;
        *head = row;
        self.order_stale = true;
        if record.template.darken != 0.0 {
            self.darkening += 1;
        }
        // At most one push per row per rewrite: a row is freed only with its effect's chain,
        // which a rewrite frees once, before writing that effect's rows.
        if self.patches.len() < self.patches.capacity() {
            self.patches.push(row);
        }
    }

    /// Live rows, in key order: the order the frame's decals are written in.
    #[must_use]
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    /// The rows the last rewrite wrote (their records are current): the patches the render
    /// world copies into its row buffer when it saw the previous generation.
    #[must_use]
    pub fn patches(&self) -> &[u32] {
        &self.patches
    }

    /// Rewrites so far. The render world applies [`Self::patches`] only on top of the previous
    /// generation; after a gap (a missed frame, a clear) it copies every live row.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// What a copy that last took generation `applied` (`None`: never) copies now.
    #[must_use]
    pub fn copy_since(&self, applied: Option<u64>) -> RowCopy {
        match applied {
            Some(applied) if applied == self.generation => RowCopy::Nothing,
            Some(applied) if applied + 1 == self.generation => RowCopy::Patches,
            _ => RowCopy::Every,
        }
    }

    /// Changes of the order so far: the render world copies [`Self::order`] when it moved.
    #[must_use]
    pub const fn order_generation(&self) -> u64 {
        self.order_generation
    }

    /// Live rows whose decal darkens what lies under it (the renderer queues its darkening
    /// item while there is one).
    #[must_use]
    pub const fn darkening(&self) -> u32 {
        self.darkening
    }

    /// Ages every row of the order at `now` from `camera` into `out`, one decal per row in
    /// order, a row that draws nothing as [`hidden_decal`]: what the compute pass writes, and
    /// what [`super::render::EffectDecalSource::Cpu`] uploads in its place.
    pub fn age_rows(
        &self,
        now: f64,
        catalog: &EffectCatalog,
        camera: Option<&CameraView>,
        out: &mut Vec<DecalInstance>,
    ) {
        out.clear();
        out.extend(self.order.iter().map(|&row| {
            age_decal(&self.records[row as usize], now, catalog, camera)
                .unwrap_or_else(hidden_decal)
        }));
    }

    /// The decals the rows draw at `now` from `camera`, in order, each with its effect's pool
    /// slot: the CPU reference of a frame's decals for tests and tools (D-15).
    pub fn evaluate(
        &self,
        now: f64,
        catalog: &EffectCatalog,
        camera: Option<&CameraView>,
        out: &mut Vec<(u32, DecalInstance)>,
    ) {
        out.clear();
        out.extend(self.order.iter().filter_map(|&row| {
            let record = &self.records[row as usize];
            age_decal(record, now, catalog, camera).map(|decal| (record.slot, decal))
        }));
    }

    /// The record of `row`.
    #[must_use]
    pub fn record(&self, row: u32) -> Option<&DecalRecord> {
        self.records.get(row as usize)
    }

    /// The key of `row`.
    #[must_use]
    pub fn key(&self, row: u32) -> Option<u64> {
        self.keys.get(row as usize).copied()
    }

    /// Live rows.
    #[must_use]
    pub fn live(&self) -> usize {
        self.order.len()
    }

    /// The live rows' keys and records in key order (tests: an incremental table equals one
    /// rebuilt from scratch).
    pub fn rows(&self) -> impl Iterator<Item = (u64, &DecalRecord)> {
        self.order
            .iter()
            .map(|row| (self.keys[*row as usize], &self.records[*row as usize]))
    }

    /// Address and capacity of every retained vector, which an allocation would change.
    #[must_use]
    pub fn buffers(&self) -> [(usize, usize); 10] {
        [
            (self.records.as_ptr() as usize, self.records.capacity()),
            (self.keys.as_ptr() as usize, self.keys.capacity()),
            (self.next.as_ptr() as usize, self.next.capacity()),
            (self.head.as_ptr() as usize, self.head.capacity()),
            (self.free.as_ptr() as usize, self.free.capacity()),
            (self.order.as_ptr() as usize, self.order.capacity()),
            (
                self.dirty_bits.as_ptr() as usize,
                self.dirty_bits.capacity(),
            ),
            (self.dirty.as_ptr() as usize, self.dirty.capacity()),
            (self.expiry.as_ptr() as usize, self.expiry.capacity()),
            (self.patches.as_ptr() as usize, self.patches.capacity()),
        ]
    }
}

/// Bits of a key's fields, from the top: pass (1), slot (20), layer (10), placement (10),
/// instance (20).
const SLOT_SHIFT: u32 = 40;
/// Mask of a key's slot field after shifting.
const SLOT_MASK: u64 = (1 << 20) - 1;

/// The order key of instance `index` of placement `placement` of layer `layer` of the effect
/// in `slot`, written in the `Detail` pass when `detail`.
#[must_use]
pub fn row_key(detail: bool, slot: u32, layer: u32, placement: u32, index: u32) -> u64 {
    (u64::from(detail) << 60)
        | (u64::from(slot) & SLOT_MASK) << SLOT_SHIFT
        | (u64::from(layer) & 0x3FF) << 30
        | (u64::from(placement) & 0x3FF) << 20
        | u64::from(index) & 0xF_FFFF
}

/// Writes the rows of the effect `instance` in `slot` into `table`: every placed, seen
/// instance of its decal layers drawn under `only` that has not ended at `now`, while it holds
/// decal slots; records when its rows next change.
fn write_rows(
    table: &mut DecalTable,
    slot: u32,
    instance: &EffectInstance,
    catalog: &EffectCatalog,
    (now, only): (f64, Option<Priority>),
) {
    let Some(effect) = catalog.effect(instance.effect) else {
        return;
    };
    if instance.decal_slots == 0 {
        return;
    }
    let age = seconds(now, instance.birth);
    let (mut until_age, mut until_exit) = (f32::INFINITY, f32::INFINITY);
    let mut grant = instance.decal_slots;
    for (layer_index, layer) in effect.layers.iter().enumerate() {
        let Block::Decal(_) = &layer.block else {
            continue;
        };
        if layer.block.pool() != Pool::Decals || only.is_some_and(|only| only != layer.priority) {
            continue;
        }
        let held = layer.held && instance.held;
        if layer_ended(layer, instance, age) {
            continue;
        }
        if held {
            if let Some(released) = instance.released_at {
                if seconds(now, released) / layer.exit.max(1e-3) >= 1.0 {
                    continue;
                }
                until_exit = until_exit.min(layer.exit.max(1e-3));
            }
        } else {
            until_age = until_age.min(super::runtime::layer_end_age(layer, instance));
        }
        write_layer_rows(table, slot, instance, (layer, layer_index), &mut grant);
    }
    if let Some(expiry) = table.expiry.get_mut(slot as usize) {
        *expiry = (until_age, until_exit);
    }
}

/// Writes the rows of one decal layer of `instance`, at most `grant` of them (the decal slots
/// its effect has left of its grant, decreased by each row written); a placed decal past the
/// grant gets no row and is counted in [`DecalTable::failed`].
fn write_layer_rows(
    table: &mut DecalTable,
    slot: u32,
    instance: &EffectInstance,
    (layer, layer_index): (&Layer, usize),
    grant: &mut u32,
) {
    let Block::Decal(look) = &layer.block else {
        return;
    };
    let detail = layer.priority == Priority::Detail;
    let layer_salt = u32::try_from(layer_index).unwrap_or(0);
    let bound: &BoundValues = &instance.bound;
    for (placement_index, placement) in layer.placements.iter().enumerate() {
        let placement_salt = u32::try_from(placement_index).unwrap_or(0);
        for index in 0..instance_count(placement, bound, instance.line) {
            if !placed_seen(placement, index, instance.line, bound) {
                continue;
            }
            if *grant == 0 {
                table.failed += 1;
                continue;
            }
            *grant -= 1;
            let seed = placed_seed(instance.seed, layer_salt, placement_salt, index);
            let placed = place(placement, index, seed, bound, instance.line);
            let position = instance.anchor + placed.offset;
            let jitter = 1.0 + layer.size_jitter * (2.0 * unit_float(hash3(seed, 0x51CE, 0)) - 1.0);
            let random_yaw = std::f32::consts::TAU * unit_float(hash3(seed, 0x0DEC, 0));
            let clip = with_fog(
                clip_window(instance, look.clip_to_line, position),
                instance.fog_clip,
            );
            let yaw = match (look.yaw, instance.line_frame.yaw) {
                (DecalYaw::AlongLine, Some(along)) => along,
                _ => random_yaw,
            };
            let variant = variant_frame(&look.element, seed);
            let (layer_a, layer_b, frame_mix) = variant.unwrap_or((0, 0, 0.0));
            let template = DecalInstance {
                center: position.to_array(),
                yaw,
                grazing: look.grazing,
                layer_a,
                layer_b,
                frame_mix,
                ramp_row: u32::from(look.ramp.0),
                clip_min: clip[2],
                clip_max: clip[3],
                clip_axis: [clip[0], clip[1]],
                clip_feather: look.clip_feather,
                relief: look.liquid.map_or(0.0, |liquid| liquid.relief),
                sheen: look.liquid.map_or(0.0, |liquid| liquid.sheen),
                gloss: look.liquid.map_or(1.0, |liquid| liquid.gloss),
                value_noise: look.liquid.map_or(0.0, |liquid| liquid.value_noise),
                dry_below: look.liquid.map_or(0.0, |liquid| liquid.dry_below),
                outline_noise: look.outline.map_or(0.0, |outline| outline.edge_noise),
                outline_fill: look.outline.map_or(0.0, |outline| outline.fill),
                outline_soft: look.outline.map_or(0.0, |outline| outline.soft),
                darken: if look.darken { 1.0 } else { 0.0 },
                ..Surface::of(look.breakup.as_ref(), seed).decal()
            };
            let record = DecalRecord {
                template,
                slot,
                birth: instance.birth,
                start: layer.start,
                delay: placed.delay,
                span: layer.span(bound),
                held: layer.held && instance.held,
                released_at: instance.released_at,
                exit: layer.exit,
                detail,
                detail_retired_at: instance.detail_retired_at,
                marks_retired_at: instance.marks_retired_at,
                curve: layer.curve,
                size_curve: layer.size_curve,
                erosion_curve: look.erosion,
                ramp_curve: layer.ramp_curve,
                strength: layer_strength(layer, bound),
                size: resolve_value(&layer.size, bound),
                size_strength: layer_size_strength(layer, bound),
                jitter,
                depth: look.depth,
                width: look.width,
                flipbook: variant.is_none().then_some(look.element),
            };
            let key = row_key(detail, slot, layer_salt, placement_salt, index);
            table.push(slot, key, &record);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a failed test is a panic")]
mod tests {
    use super::*;
    use crate::effects::catalog::schema::{Binding, EventKind};
    use crate::effects::catalog::{CurveId, EffectId, ElementRef, Pool};
    use crate::effects::placement::WorldLine;
    use crate::effects::pool::EffectHandle;
    use crate::effects::runtime::{EffectRuntime, SpawnRequest};
    use crate::effects::test_streams::{
        run_stream_with, shipped_catalog, stream_camera, StreamKind,
    };

    /// The rows of `runtime`'s table rebuilt from scratch at `now`.
    fn scratch(runtime: &EffectRuntime, catalog: &EffectCatalog, now: f64) -> DecalTable {
        let pool = runtime.pool();
        let mut table = DecalTable::with_capacity(catalog.budget.decals, pool.capacity());
        table.mark_all();
        table.rewrite(pool, catalog, now, runtime.only());
        table
    }

    #[test]
    fn the_incremental_table_equals_one_rebuilt_from_scratch_over_both_streams() {
        let budgets: [fn(&mut EffectCatalog); 2] = [
            |_| {},
            |catalog| {
                catalog.budget.decals = 600;
                catalog.budget.class_caps = [4, 12, 4];
            },
        ];
        for budget in budgets {
            for kind in [StreamKind::Splashes, StreamKind::Plan] {
                let mut buffers = None;
                let mut rows = 0;
                run_stream_with(kind, 900, budget, |catalog, runtime, now| {
                    let table = runtime.decal_table();
                    let fresh = scratch(runtime, catalog, now);
                    assert!(
                        table.rows().eq(fresh.rows()),
                        "{kind:?} at {now}: {} rows against {} from scratch",
                        table.live(),
                        fresh.live()
                    );
                    assert_eq!(table.failed, 0, "every row found room");
                    for (slot, instance) in runtime.pool().iter() {
                        let held = table
                            .rows()
                            .filter(|(_, record)| record.slot as usize == slot)
                            .count();
                        assert!(
                            held <= instance.decal_slots as usize,
                            "an effect's rows never exceed its granted decal slots"
                        );
                    }
                    let now_buffers = table.buffers();
                    assert_eq!(
                        *buffers.get_or_insert(now_buffers),
                        now_buffers,
                        "no growth"
                    );
                    rows += table.live();
                });
                assert!(rows > 0, "{kind:?}: the stream has decal rows");
            }
        }
    }

    /// The first effect of the vocabulary of one of `kinds` with a decal layer.
    fn decal_effect(catalog: &EffectCatalog, kinds: &[EventKind]) -> EffectId {
        let index = catalog
            .effects
            .iter()
            .position(|effect| {
                kinds.contains(&effect.event)
                    && effect
                        .layers
                        .iter()
                        .any(|layer| layer.block.pool() == Pool::Decals)
            })
            .expect("a shipped decal look of that kind");
        EffectId(u16::try_from(index).expect("a small vocabulary"))
    }

    /// A request for `effect` at `anchor`, with a line along +x when `line`.
    fn request(effect: EffectId, anchor: Vec3, line: Option<u64>) -> SpawnRequest {
        let bound = BoundValues::default()
            .with(Binding::SplashFull, 0.6)
            .with(Binding::SplashMid, 0.9)
            .with(Binding::SplashReach, 1.2)
            .with(Binding::LineSpacing, 0.4)
            .with(Binding::LineLength, 4.0)
            .with(Binding::AreaRadius, 1.5)
            .with(Binding::UnitRadius, 0.5);
        SpawnRequest {
            effect,
            anchor,
            bound,
            line: line.map(|seen| WorldLine {
                from: anchor,
                to: anchor + Vec3::X * 4.0,
                seen,
            }),
            delay: 0.0,
        }
    }

    /// A runtime with a lasting mark, a line and a held look, built for a few frames.
    struct Scene {
        catalog: EffectCatalog,
        runtime: EffectRuntime,
        mark: EffectHandle,
        held: EffectHandle,
        now: f64,
    }

    impl Scene {
        fn new(edit: impl FnOnce(&mut EffectCatalog)) -> Self {
            let mut catalog = shipped_catalog();
            edit(&mut catalog);
            let mut runtime = EffectRuntime::new(&catalog);
            let splash = decal_effect(&catalog, &[EventKind::Splash, EventKind::Point]);
            let mark = runtime
                .spawn(
                    &catalog,
                    &request(splash, Vec3::new(20.0, 0.0, 20.0), None),
                    0.0,
                )
                .expect("spawned");
            let looks = decal_effect(
                &catalog,
                &[EventKind::Status, EventKind::Mode, EventKind::Area],
            );
            let held = runtime
                .hold(
                    &catalog,
                    &request(looks, Vec3::new(24.0, 0.0, 20.0), Some(u64::MAX)),
                    0.0,
                    false,
                )
                .expect("held");
            let mut scene = Self {
                catalog,
                runtime,
                mark,
                held,
                now: 0.0,
            };
            for _ in 0..12 {
                scene.frame();
            }
            scene
        }

        fn frame(&mut self) {
            self.now += 1.0 / 60.0;
            self.runtime
                .build_frame(&self.catalog, self.now, Some(&stream_camera()));
        }

        /// The next frame's decals equal those of a copy whose table is rebuilt from scratch.
        fn assert_next_frame_matches_scratch(&mut self, api: &str) {
            self.frame();
            let fresh = scratch(&self.runtime, &self.catalog, self.now);
            let camera = stream_camera();
            let (expected, owners): (Vec<DecalInstance>, Vec<u32>) = fresh
                .rows()
                .filter_map(|(_, record)| {
                    age_decal(record, self.now, &self.catalog, Some(&camera))
                        .map(|decal| (decal, record.slot))
                })
                .unzip();
            assert!(!expected.is_empty(), "{api}: the scene draws decals");
            let bits =
                |decals: &[DecalInstance]| -> Vec<u8> { bytemuck::cast_slice(decals).to_vec() };
            let mut evaluated = Vec::new();
            self.runtime
                .evaluate_decals(&self.catalog, self.now, Some(&camera), &mut evaluated);
            let (decals, slots): (Vec<DecalInstance>, Vec<u32>) = evaluated
                .into_iter()
                .map(|(slot, decal)| (decal, slot))
                .unzip();
            assert_eq!(
                bits(&decals),
                bits(&expected),
                "{api}: the retained rows draw what rows rebuilt from scratch draw"
            );
            assert_eq!(slots, owners);
        }
    }

    #[test]
    fn spawn_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        let splash = decal_effect(&scene.catalog, &[EventKind::Splash, EventKind::Point]);
        scene.runtime.spawn(
            &scene.catalog,
            &request(splash, Vec3::new(18.0, 0.0, 22.0), None),
            scene.now,
        );
        scene.assert_next_frame_matches_scratch("spawn");
    }

    #[test]
    fn hold_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        let looks = decal_effect(
            &scene.catalog,
            &[EventKind::Status, EventKind::Mode, EventKind::Area],
        );
        scene.runtime.hold(
            &scene.catalog,
            &request(looks, Vec3::new(16.0, 0.0, 18.0), None),
            scene.now,
            true,
        );
        scene.assert_next_frame_matches_scratch("hold");
    }

    #[test]
    fn merge_line_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        let line = decal_effect(&scene.catalog, &[EventKind::Line]);
        let anchor = Vec3::new(14.0, 0.0, 24.0);
        scene
            .runtime
            .spawn(&scene.catalog, &request(line, anchor, Some(0b1)), scene.now);
        scene.frame();
        let merged = scene.runtime.spawn(
            &scene.catalog,
            &request(line, anchor, Some(0b1110)),
            scene.now,
        );
        assert!(merged.is_none(), "the second request joins the line");
        scene.assert_next_frame_matches_scratch("merge_line");
    }

    #[test]
    fn update_held_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        let bound = request(EffectId(0), Vec3::ZERO, None)
            .bound
            .with(Binding::UnitRadius, 0.9);
        assert!(scene
            .runtime
            .update_held(scene.held, Vec3::new(25.0, 0.0, 21.0), bound));
        scene.assert_next_frame_matches_scratch("update_held");
    }

    #[test]
    fn update_held_line_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        let from = Vec3::new(23.0, 0.0, 19.0);
        let line = WorldLine::whole(from, from + Vec3::new(3.0, 0.0, 2.0));
        let bound = request(EffectId(0), Vec3::ZERO, None).bound;
        assert!(scene.runtime.update_held_line(scene.held, line, bound));
        scene.assert_next_frame_matches_scratch("update_held_line");
    }

    #[test]
    fn move_anchor_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        assert!(scene
            .runtime
            .move_anchor(scene.mark, Vec3::new(21.0, 0.0, 19.5)));
        scene.assert_next_frame_matches_scratch("move_anchor");
    }

    #[test]
    fn set_fog_clip_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        scene
            .runtime
            .set_fog_clip(scene.mark, [1.0, 0.0, 20.0, 30.0]);
        scene.assert_next_frame_matches_scratch("set_fog_clip");
    }

    #[test]
    fn release_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        scene.runtime.release(scene.held, scene.now);
        scene.frame();
        scene.assert_next_frame_matches_scratch("release");
    }

    #[test]
    fn retire_marks_its_rows() {
        let mut scene = Scene::new(|_| {});
        scene.runtime.retire(scene.mark);
        scene.assert_next_frame_matches_scratch("retire");
    }

    #[test]
    fn detail_retirement_marks_its_rows() {
        let mut scene = Scene::new(|catalog| catalog.budget.class_caps = [1, 1, 1]);
        // A look whose lasting mark is a `Detail` layer, so its detail retirement fades it.
        let index = scene
            .catalog
            .effects
            .iter()
            .position(|effect| {
                effect.layers.iter().any(|layer| {
                    layer.block.pool() == Pool::Decals && layer.priority == Priority::Detail
                }) && matches!(effect.event, EventKind::Splash | EventKind::Point)
            })
            .expect("a shipped look with a detail mark");
        let victim = EffectId(u16::try_from(index).expect("a small vocabulary"));
        scene.runtime.retire(scene.mark);
        scene.frame();
        let at = Vec3::new(17.0, 0.0, 17.0);
        scene
            .runtime
            .spawn(&scene.catalog, &request(victim, at, None), scene.now);
        for _ in 0..6 {
            scene.frame();
        }
        let before = scene.runtime.stats.retired;
        scene.runtime.spawn(
            &scene.catalog,
            &request(victim, at + Vec3::X * 3.0, None),
            scene.now,
        );
        assert!(scene.runtime.stats.retired > before, "a detail was retired");
        scene.assert_next_frame_matches_scratch("detail retirement");
    }

    #[test]
    fn mark_retirement_marks_its_rows() {
        let mut scene = Scene::new(|catalog| catalog.budget.decals = 40);
        let splash = decal_effect(&scene.catalog, &[EventKind::Splash, EventKind::Point]);
        for step in 0_u8..40 {
            scene.runtime.spawn(
                &scene.catalog,
                &request(
                    splash,
                    Vec3::new(10.0 + f32::from(step % 8), 0.0, 15.0 + f32::from(step / 8)),
                    None,
                ),
                scene.now,
            );
        }
        assert!(scene.runtime.stats.marks_retired > 0, "marks were retired");
        scene.assert_next_frame_matches_scratch("mark retirement");
    }

    /// D-8: aging through the split times gives the `f64` difference cast to `f32`, or a
    /// neighbouring `f32` in rare cases (about 1 %), over session times up to a day.
    #[test]
    fn split_seconds_equal_the_f64_difference_within_one_ulp() {
        let mut state = 0x5EED_u64;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(state >> 32).expect("32 bits")) / f64::from(u32::MAX)
        };
        let (mut exact, mut total) = (0_u32, 0_u32);
        for _ in 0..200_000 {
            let now = next() * 86_400.0;
            // Events from the last minute (the table's rows), and from the session's start.
            let since = if next() < 0.9 {
                now - next() * 60.0
            } else {
                next() * now
            };
            #[expect(clippy::cast_possible_truncation, reason = "the reference cast")]
            let reference = (now - since) as f32;
            let split = split_seconds(split_time(now), split_time(since));
            let ulps = (i64::from(reference.to_bits().cast_signed())
                - i64::from(split.to_bits().cast_signed()))
            .abs();
            assert!(
                ulps <= 1,
                "{now} - {since}: {split} against {reference} ({ulps} ulp)"
            );
            exact += u32::from(ulps == 0);
            total += 1;
        }
        // Measured: 197 766 of 200 000 exact (98.9 %); the rest one ulp apart.
        assert!(
            exact * 100 >= total * 98,
            "all but rare cases are exact: {exact} of {total}"
        );
        let time = 1_234.567_890_123;
        let [hi, lo] = split_time(time);
        assert!(
            (f64::from(hi) + f64::from(lo) - time).abs() < 1e-11,
            "the rest carries what the nearest f32 leaves"
        );
    }

    /// D-5: a copy fed the table's patches after every rewrite, and every live row after a
    /// missed frame, holds the table's rows, so the GPU's row buffer does.
    #[test]
    fn a_copy_fed_the_patches_holds_the_tables_rows_over_both_streams() {
        for kind in [StreamKind::Splashes, StreamKind::Plan] {
            let mut copy: Vec<GpuDecalRecord> = Vec::new();
            let mut applied: Option<u64> = None;
            let (mut frame, mut patched, mut live, mut resyncs) = (0_u32, 0_usize, 0_usize, 0);
            run_stream_with(
                kind,
                900,
                |_| {},
                |catalog, runtime, _| {
                    let table = runtime.decal_table();
                    if copy.is_empty() {
                        copy = vec![GpuDecalRecord::zeroed(); catalog.budget.decals as usize];
                    }
                    frame += 1;
                    // Every 97th frame the copy misses its extraction.
                    if frame % 97 == 0 {
                        return;
                    }
                    let rows: &[u32] = match table.copy_since(applied) {
                        RowCopy::Nothing => &[],
                        RowCopy::Patches => table.patches(),
                        RowCopy::Every => {
                            resyncs += 1;
                            table.order()
                        }
                    };
                    for &row in rows {
                        copy[row as usize] =
                            GpuDecalRecord::of(table.record(row).expect("a written row"));
                    }
                    applied = Some(table.generation());
                    patched += table.patches().len();
                    live += table.live();
                    for &row in table.order() {
                        let record = table.record(row).expect("a live row");
                        assert_eq!(
                            copy[row as usize],
                            GpuDecalRecord::of(record),
                            "{kind:?}, frame {frame}: row {row} is current"
                        );
                    }
                    let darkening = table
                        .rows()
                        .filter(|(_, record)| record.template.darken != 0.0)
                        .count();
                    assert_eq!(table.darkening() as usize, darkening, "darkening rows");
                },
            );
            println!(
                "{kind:?}: {patched} rows patched against {live} live row-frames, {resyncs} \
                 full copies"
            );
            assert!(
                resyncs >= 9,
                "{kind:?}: every missed frame forced a full copy"
            );
            assert!(
                patched * 5 < live,
                "{kind:?}: patches are a small share of the rows ({patched} of {live})"
            );
        }
    }

    /// A clear leaves a gap in the generations, so the next copy takes every row.
    #[test]
    fn a_clear_makes_the_next_copy_take_every_row() {
        let mut scene = Scene::new(|_| {});
        let generation = scene.runtime.decal_table().generation();
        assert_eq!(
            scene.runtime.decal_table().copy_since(Some(generation)),
            RowCopy::Nothing
        );
        scene.runtime.clear();
        scene.frame();
        assert_eq!(
            scene.runtime.decal_table().copy_since(Some(generation)),
            RowCopy::Every
        );
        assert_eq!(scene.runtime.decal_table().copy_since(None), RowCopy::Every);
    }

    /// Amendment 1's open item: an effect never has more rows than the decal slots it was
    /// granted; a placed decal past the grant is counted, not given a row.
    #[test]
    fn an_effects_rows_never_exceed_its_grant() {
        let catalog = shipped_catalog();
        let mut runtime = EffectRuntime::new(&catalog);
        for (index, effect) in catalog.effects.iter().enumerate() {
            if effect
                .layers
                .iter()
                .any(|layer| layer.block.pool() == Pool::Decals)
                && matches!(
                    effect.event,
                    EventKind::Splash | EventKind::Point | EventKind::Line
                )
            {
                let id = EffectId(u16::try_from(index).expect("a small vocabulary"));
                let anchor = Vec3::new(4.0 * f32::from(id.0 % 8), 0.0, 4.0 * f32::from(id.0 / 8));
                runtime.spawn(&catalog, &request(id, anchor, Some(u64::MAX)), 0.0);
            }
        }
        runtime.build_frame(&catalog, 0.05, None);
        let table = runtime.decal_table();
        let (slot, rows) = (0..runtime.pool().capacity())
            .map(|slot| (slot, table.rows().filter(|(_, r)| r.slot == slot).count()))
            .max_by_key(|(_, rows)| *rows)
            .expect("a slot");
        assert!(rows >= 2, "a shipped look places several decals");
        let mut pool = runtime.pool().clone();
        pool.slot_mut(slot as usize).expect("live").decal_slots = 1;
        let mut fresh = DecalTable::with_capacity(catalog.budget.decals, pool.capacity());
        fresh.mark_all();
        fresh.rewrite(&pool, &catalog, 0.05, None);
        let kept = fresh.rows().filter(|(_, r)| r.slot == slot).count();
        assert_eq!(kept, 1, "the rows stop at the grant");
        assert_eq!(
            fresh.failed,
            (rows - 1) as u64,
            "every decal past it is counted"
        );
    }

    /// The rows aged in order hold one decal per row, a row that draws nothing as the zero
    /// decal, and the rest are the reference's decals in order.
    #[test]
    fn aged_rows_align_with_the_order_and_hide_what_draws_nothing() {
        let (mut hidden, mut drawn) = (0, 0);
        run_stream_with(
            StreamKind::Plan,
            300,
            |_| {},
            |catalog, runtime, now| {
                let table = runtime.decal_table();
                let camera = stream_camera();
                let (mut aged, mut evaluated) = (Vec::new(), Vec::new());
                table.age_rows(now, catalog, Some(&camera), &mut aged);
                table.evaluate(now, catalog, Some(&camera), &mut evaluated);
                assert_eq!(aged.len(), table.order().len());
                let shown: Vec<DecalInstance> = aged
                    .iter()
                    .copied()
                    .filter(|d| *d != hidden_decal())
                    .collect();
                let expected: Vec<DecalInstance> = evaluated.iter().map(|(_, d)| *d).collect();
                assert_eq!(shown, expected);
                hidden += aged.len() - shown.len();
                drawn += shown.len();
            },
        );
        assert!(
            drawn > 0 && hidden > 0,
            "both kinds occur: {drawn} drawn, {hidden} hidden"
        );
        assert!(bytemuck::bytes_of(&hidden_decal())
            .iter()
            .all(|byte| *byte == 0));
    }

    /// The GPU form carries every input: options as flags, times split.
    #[test]
    fn gpu_records_carry_every_input() {
        let catalog = shipped_catalog();
        let scene = Scene::new(|_| {});
        let record = *scene.runtime.decal_table().rows().next().expect("a row").1;
        let plain = GpuDecalRecord::of(&DecalRecord {
            held: false,
            detail: false,
            released_at: None,
            detail_retired_at: None,
            marks_retired_at: None,
            flipbook: None,
            curve: None,
            ..record
        });
        assert_eq!(plain.flags, 0);
        assert_eq!(plain.curves[0], NO_CURVE);
        assert_eq!(plain.birth, split_time(record.birth));
        let full = GpuDecalRecord::of(&DecalRecord {
            held: true,
            detail: true,
            released_at: Some(1.5),
            detail_retired_at: Some(2.5),
            marks_retired_at: Some(3.5),
            flipbook: Some(ElementRef {
                first_layer: 7,
                frames: 4,
                duration_secs: None,
            }),
            curve: Some(CurveId(3)),
            ..record
        });
        assert_eq!(full.flags, 63, "every flag");
        assert_eq!(
            (full.released, full.detail_retired, full.marks_retired),
            ([1.5, 0.0], [2.5, 0.0], [3.5, 0.0])
        );
        assert_eq!((full.flipbook_first, full.flipbook_frames), (7, 4));
        assert_eq!(full.curves[0], 3);
        let (ranges, keys) = curve_table(&catalog);
        assert_eq!(ranges.len(), catalog.curves.len());
        for (range, curve) in ranges.iter().zip(&catalog.curves) {
            let [first, count] = *range;
            let span = &keys[first as usize..(first + count) as usize];
            assert!(span
                .iter()
                .zip(curve.keys())
                .all(|(gpu, key)| gpu.time.to_bits() == key.0.to_bits()
                    && gpu.value.to_bits() == key.1.to_bits()));
        }
    }

    #[test]
    fn only_marks_every_row() {
        let mut scene = Scene::new(|_| {});
        scene.runtime.set_only(Some(Priority::Critical));
        scene.assert_next_frame_matches_scratch("only");
    }
}
