//! Bounded indices of the live effects for the spawn path (plan
//! `2026-11-19-retained-effect-instances` D-4).
//!
//! # Responsibility
//! Owns [`RecentLines`], the live line effects a new line request may join, kept in slot order,
//! and [`OldestHeap`], a min-heap of live effects by `(birth, slot)` with lazy deletion. The
//! runtime keeps one heap per budget class for the oldest burning effect, one for the oldest
//! lasting marks and one for the oldest detail, so a spawn over a cap or above the decal
//! pressure mark finds its effect without walking the pool's 6 144 slots.
//!
//! # How it fits
//! [`super::runtime::EffectRuntime`] adds every placed effect to the indices and asks them in
//! `merge_line`, `retire_oldest_fire`, `retire_oldest_mark` and `retire_one`; whether an entry
//! still qualifies is decided by the caller's predicate on the live instance. Every condition
//! the predicates test only ever turns from true to false while time runs forward (a retired
//! detail stays retired, a fire that ended stays ended, decal slots only fall, a slot reused by
//! another effect has another generation), so an entry that fails once is dropped for good.
//!
//! # Invariants
//! - Each index is allocated once at the pool's capacity and never grows: an index that is
//!   full is first compacted to its live entries, and each live effect has at most one entry.
//! - The heap's order key `(birth, slot)` reproduces `min_by(birth.total_cmp)` over the slots in
//!   ascending order, which returns the lowest slot among equal births.
//! - `RecentLines` scans its entries in ascending slot order, the order of the pool scan it
//!   replaces, so the first match is the same effect.

use super::pool::{EffectHandle, EffectInstance, EffectInstancePool};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// An `f64` mapped to a `u64` whose integer order is `f64::total_cmp`'s order.
#[must_use]
pub const fn total_order_key(value: f64) -> u64 {
    let bits = value.to_bits();
    if bits >> 63 == 1 {
        !bits
    } else {
        bits | (1 << 63)
    }
}

/// A min-heap of live effects by `(birth, slot)` with lazy deletion.
// PERFORMANCE RATIONALE:
// Why: A spawn over its class cap, a mark retirement above the decal pressure mark and every
// frame whose detail overflowed each needed the oldest qualifying effect; a scan of the pool's
// 6 144 slots per call cost a large share of the spawn path at 200 splashes a second (B-137).
// What: A binary heap of `(birth key, slot, generation)` entries preallocated at the pool's
// capacity; entries that no longer qualify are popped when they reach the top.
// Trade-off: A second structure kept in step with the pool (an entry per placed effect, a
// generation check per pop) in exchange for O(log n) per spawn and per retirement.
#[derive(Debug, Clone)]
pub struct OldestHeap {
    /// Entries `(birth key, slot, generation)`, smallest on top.
    heap: BinaryHeap<Reverse<(u64, u32, u32)>>,
    /// The pool's capacity.
    capacity: usize,
}

/// Two heaps are equal when they hold the same entries in the same layout.
impl PartialEq for OldestHeap {
    fn eq(&self, other: &Self) -> bool {
        self.capacity == other.capacity && self.heap.as_slice() == other.heap.as_slice()
    }
}

impl Eq for OldestHeap {}

impl OldestHeap {
    /// An empty heap for a pool of `capacity` slots.
    #[must_use]
    pub fn with_capacity(capacity: u32) -> Self {
        let capacity = capacity as usize;
        Self {
            heap: BinaryHeap::with_capacity(capacity),
            capacity,
        }
    }

    /// Adds the effect `handle`, born at `birth`. A full heap is first compacted to the entries
    /// of live effects in `pool`.
    pub fn push(&mut self, handle: EffectHandle, birth: f64, pool: &EffectInstancePool) {
        if self.heap.len() >= self.capacity {
            self.heap.retain(|Reverse((_, slot, generation))| {
                pool.get(EffectHandle {
                    index: *slot,
                    generation: *generation,
                })
                .is_some()
            });
        }
        if self.heap.len() < self.capacity {
            self.heap.push(Reverse((
                total_order_key(birth),
                handle.index,
                handle.generation,
            )));
        }
    }

    /// The oldest live effect of `pool` for which `qualifies` holds, lowest slot first among
    /// equal births; the entries above it that no longer qualify are dropped.
    pub fn oldest(
        &mut self,
        pool: &EffectInstancePool,
        qualifies: impl Fn(&EffectInstance) -> bool,
    ) -> Option<EffectHandle> {
        while let Some(&Reverse((_, slot, generation))) = self.heap.peek() {
            let handle = EffectHandle {
                index: slot,
                generation,
            };
            if pool.get(handle).is_some_and(&qualifies) {
                return Some(handle);
            }
            self.heap.pop();
        }
        None
    }

    /// Removes every entry.
    pub fn clear(&mut self) {
        self.heap.clear();
    }

    /// Entries, live or not.
    #[must_use]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Whether the heap has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Address and capacity of the retained storage, which an allocation would change.
    #[must_use]
    pub fn buffer(&self) -> (usize, usize) {
        (self.heap.as_slice().as_ptr() as usize, self.heap.capacity())
    }
}

/// The live line effects a line request may still join, in ascending slot order.
// PERFORMANCE RATIONALE:
// Why: Every line request looked for the live effect of its line by scanning the pool's 6 144
// slots, although only an effect younger than its merge window (about a second) can match.
// What: A list of the handles of live line effects, preallocated at the pool's capacity, pruned
// of effects past their window once a frame and kept sorted by slot.
// Trade-off: A second structure kept in step with the pool and a sort of a few hundred handles
// on frames that added lines, in exchange for scanning the last second's lines only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentLines {
    /// Handles, sorted by slot when `sorted`.
    entries: Vec<EffectHandle>,
    /// Whether `entries` is in ascending slot order.
    sorted: bool,
}

impl RecentLines {
    /// An empty list for a pool of `capacity` slots.
    #[must_use]
    pub fn with_capacity(capacity: u32) -> Self {
        Self {
            entries: Vec::with_capacity(capacity as usize),
            sorted: true,
        }
    }

    /// Adds the live line effect `handle`. A full list is first pruned of effects no longer live
    /// in `pool`.
    pub fn push(&mut self, handle: EffectHandle, pool: &EffectInstancePool) {
        if self.entries.len() == self.entries.capacity() {
            self.entries.retain(|entry| pool.get(*entry).is_some());
        }
        if self.entries.len() < self.entries.capacity() {
            self.entries.push(handle);
            self.sorted = false;
        }
    }

    /// Drops the entries of `pool` that are no longer live or for which `keep` fails.
    pub fn prune(&mut self, pool: &EffectInstancePool, keep: impl Fn(&EffectInstance) -> bool) {
        self.entries
            .retain(|entry| pool.get(*entry).is_some_and(&keep));
    }

    /// The entries in ascending slot order.
    pub fn in_slot_order(&mut self) -> &[EffectHandle] {
        if !self.sorted {
            self.entries.sort_unstable_by_key(|handle| handle.index);
            self.sorted = true;
        }
        &self.entries
    }

    /// Removes every entry.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.sorted = true;
    }

    /// Address and capacity of the retained storage, which an allocation would change.
    #[must_use]
    pub fn buffer(&self) -> (usize, usize) {
        (self.entries.as_ptr() as usize, self.entries.capacity())
    }
}

#[cfg(test)]
pub mod tests {
    //! The pool scans the indices replaced, kept as oracles (plan D-4), and the indices' own
    //! tests.
    use super::*;
    use crate::effects::catalog::schema::EffectClass;
    use crate::effects::test_streams::{run_stream_with, StreamKind};

    /// The scan `retire_oldest_fire` made: the oldest burning effect of `class` with detail.
    #[must_use]
    pub fn oracle_oldest_fire(
        pool: &EffectInstancePool,
        class: EffectClass,
        now: f64,
    ) -> Option<usize> {
        pool.iter()
            .filter(|(_, instance)| {
                instance.class == class
                    && !instance.held
                    && instance.has_detail
                    && instance.detail_retired_at.is_none()
                    && now < instance.fire_end
            })
            .min_by(|(_, a), (_, b)| a.birth.total_cmp(&b.birth))
            .map(|(index, _)| index)
    }

    /// The scan `retire_oldest_mark` made.
    #[must_use]
    pub fn oracle_oldest_mark(pool: &EffectInstancePool) -> Option<usize> {
        pool.iter()
            .filter(|(_, instance)| {
                instance.decal_slots > 0 && instance.marks_retired_at.is_none() && !instance.held
            })
            .min_by(|(_, a), (_, b)| a.birth.total_cmp(&b.birth))
            .map(|(index, _)| index)
    }

    /// The scan `retire_one` made through `budget::retirement_candidate`.
    #[must_use]
    pub fn oracle_oldest_detail(pool: &EffectInstancePool) -> Option<usize> {
        crate::effects::budget::retirement_candidate(pool.iter().map(|(index, instance)| {
            (
                index,
                crate::effects::budget::RetireCandidate {
                    class: instance.class,
                    birth: instance.birth,
                    detail_retired: instance.detail_retired_at.is_some(),
                    has_detail: instance.has_detail && !instance.held,
                },
            )
        }))
    }

    /// The scan `merge_line` made: the first slot whose effect `matches`.
    pub fn oracle_merge(
        pool: &EffectInstancePool,
        matches: impl Fn(&EffectInstance) -> bool,
    ) -> Option<usize> {
        pool.iter()
            .find(|(_, instance)| matches(instance))
            .map(|(index, _)| index)
    }

    thread_local! {
        /// Choices compared with their oracle, by kind: merge, fire, mark, detail.
        pub static COMPARED: std::cell::Cell<[u64; 4]> = const { std::cell::Cell::new([0; 4]) };
    }

    /// Counts one compared choice of `kind` (0 merge, 1 fire, 2 mark, 3 detail).
    pub fn compared(kind: usize) {
        COMPARED.with(|cell| {
            let mut counts = cell.get();
            counts[kind] += 1;
            cell.set(counts);
        });
    }

    #[test]
    fn the_total_order_key_orders_as_total_cmp() {
        let values = [
            f64::NEG_INFINITY,
            -1e9,
            -1.5,
            -0.0,
            0.0,
            1e-300,
            0.5,
            2.0,
            1e9,
            f64::INFINITY,
        ];
        for a in values {
            for b in values {
                assert_eq!(
                    total_order_key(a).cmp(&total_order_key(b)),
                    a.total_cmp(&b),
                    "{a} vs {b}"
                );
            }
        }
    }

    /// Every choice of the spawn path over both streams equals the pool scan's (the runtime
    /// asserts it under `cfg(test)` at every call), and each kind of choice was made.
    #[test]
    fn every_index_choice_equals_the_scan_over_both_streams() {
        COMPARED.with(|cell| cell.set([0; 4]));
        let mut buffers = None;
        // The shipped budget, and one tight enough that marks and detail retire.
        let budgets: [fn(&mut crate::effects::catalog::EffectCatalog); 2] = [
            |_| {},
            |catalog| {
                catalog.budget.decals = 600;
                catalog.budget.sprites = 400;
                catalog.budget.effects = 900;
            },
        ];
        for budget in budgets {
            for kind in [StreamKind::Splashes, StreamKind::Plan] {
                run_stream_with(kind, 900, budget, |_, runtime, _| {
                    let now = runtime.index_buffers();
                    assert_eq!(*buffers.get_or_insert(now), now, "no index grew");
                });
                buffers = None;
            }
        }
        let counts = COMPARED.with(std::cell::Cell::get);
        assert!(
            counts.iter().all(|count| *count > 0),
            "merge, fire, mark and detail choices were all compared: {counts:?}"
        );
    }
}
