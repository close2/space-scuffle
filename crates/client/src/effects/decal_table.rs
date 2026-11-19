//! The retained table of lasting decals (plan `2026-11-19-retained-effect-instances` D-2, D-5).
//!
//! # Responsibility
//! Owns [`DecalTable`]: one row per placed decal of a live effect, holding a [`DecalRecord`]
//! (everything fixed by its effect's state) and an order key, kept in the order the frame
//! build used to emit decals: the `Critical` pass over slots ascending, then the `Detail` pass,
//! each by layer, placement and instance. Also owns the record writer ([`write_rows`]), which
//! runs the per-instance work `push_decal` ran (placement, clip, hashes, the yaw, the surface)
//! once when an effect's decal inputs change, and [`age_decal`], the pure function of a record,
//! the time and the camera that gives the decal's [`DecalInstance`] of a frame.
//!
//! # How it fits
//! [`super::runtime::EffectRuntime`] marks an effect dirty whenever an API changes one of its
//! decal inputs (spawn, hold, a merged line, a release, a move, a fog clip, a retirement of its
//! detail or marks, its end, a change of `only`) and when one of its decal layers ends; once a
//! frame [`DecalTable::rewrite`] frees each dirty effect's rows and writes them anew. The frame
//! build then ages every row in order into the frame's decals. So the picture is the same pure
//! function of the event stream and `Time<Virtual>` as before, with the per-frame work of a
//! decal reduced to its aging.
//!
//! # Invariants
//! - Every vector is allocated at construction, at the decal budget's or the pool's capacity,
//!   and only cleared and refilled: a steady frame allocates nothing.
//! - The rows of an effect are exactly the placed, seen instances of its decal layers that are
//!   drawn by its `only` filter and have not ended, while it holds decal slots.
//! - Keys are unique, so the order is a total order; it is re-sorted only on frames whose rows
//!   changed.

use super::binding::{resolve_value, BoundValues};
use super::budget::{lod_skips, retire_fade};
use super::catalog::schema::{DecalYaw, Priority};
use super::catalog::{Block, CurveId, EffectCatalog, ElementRef, Layer, Pool};
use super::instances::DecalInstance;
use super::ledger::mark_fade;
use super::placement::{hash3, instance_count, place, placed_seen, unit_float};
use super::pool::{EffectInstance, EffectInstancePool};
use super::runtime::{
    clip_window, flipbook_frame, layer_ended, layer_size_strength, layer_strength, placed_seed,
    variant_frame, with_fog, CameraView, Surface,
};
use bevy::math::Vec3;

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

/// The clock of `record` at `now`, or `None` when it draws nothing: its effect's detail faded
/// out, not started, past its span, or eroded out after a release. The same expressions, in
/// the same order, as `write_layers`, `marks_fade` and `layer_clock`.
#[must_use]
pub fn decal_clock(record: &DecalRecord, now: f64) -> Option<DecalClock> {
    let detail_fade = match (record.detail, record.detail_retired_at) {
        (true, Some(retired)) => retire_fade(seconds(now, retired)),
        _ => 1.0,
    };
    if detail_fade <= 0.0 {
        return None;
    }
    let fade = record.marks_retired_at.map_or(detail_fade, |retired| {
        detail_fade * mark_fade(seconds(now, retired))
    });
    let since_birth = seconds(now, record.birth) - record.start;
    let released_exit = || match record.released_at {
        Some(released) => (seconds(now, released) / record.exit.max(1e-3)).max(0.0),
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

/// The retained decal rows of the live effects.
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
    /// Rows that found no free row (a sizing defect).
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

    /// Removes every row.
    pub fn clear(&mut self) {
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
    }

    /// Live rows, in key order: the order the frame's decals are written in.
    #[must_use]
    pub fn order(&self) -> &[u32] {
        &self.order
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
    pub fn buffers(&self) -> [(usize, usize); 9] {
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
        write_layer_rows(table, slot, instance, (layer, layer_index));
    }
    if let Some(expiry) = table.expiry.get_mut(slot as usize) {
        *expiry = (until_age, until_exit);
    }
}

/// Writes the rows of one decal layer of `instance`.
fn write_layer_rows(
    table: &mut DecalTable,
    slot: u32,
    instance: &EffectInstance,
    (layer, layer_index): (&Layer, usize),
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
    use crate::effects::catalog::EffectId;
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
            assert_eq!(
                bits(&self.runtime.frame.decals),
                bits(&expected),
                "{api}: the retained rows draw what rows rebuilt from scratch draw"
            );
            assert_eq!(self.runtime.frame.decal_owners, owners);
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

    #[test]
    fn only_marks_every_row() {
        let mut scene = Scene::new(|_| {});
        scene.runtime.set_only(Some(Priority::Critical));
        scene.assert_next_frame_matches_scratch("only");
    }
}
