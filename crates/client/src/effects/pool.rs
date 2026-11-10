//! The fixed-capacity pool of live effect instances (plan §5.6 "Instances").
//!
//! # Responsibility
//! Owns [`EffectInstancePool`]: generation-checked slots, a free list and the live count per
//! [`EffectClass`]. A slot holds one [`EffectInstance`]: which effect, when it was born, where
//! it is anchored, its bound values, its line, its seed and its budget state.
//!
//! # How it fits
//! The event wiring spawns into it; the runtime walks the live slots every frame to build the
//! frame's instances and frees an effect once its last layer has ended.
//!
//! # Invariants
//! - Capacity is fixed at construction and never grows: the slot and free-list vectors are
//!   allocated once, so spawning and freeing allocate nothing.
//! - A [`EffectHandle`] is valid only while its slot's generation matches.
//! - `live_by_class` equals the number of occupied slots of each class.

use super::binding::BoundValues;
use super::catalog::schema::EffectClass;
use super::catalog::EffectId;
use super::placement::WorldLine;
use bevy::math::Vec3;

/// A generation-checked reference to a pool slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectHandle {
    /// Slot index.
    pub index: u32,
    /// Generation of the slot when the handle was made.
    pub generation: u32,
}

/// One live effect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectInstance {
    /// The effect.
    pub effect: EffectId,
    /// Its budget class.
    pub class: EffectClass,
    /// Birth time, seconds of `Time<Virtual>`.
    pub birth: f64,
    /// Time after which no layer draws, seconds of `Time<Virtual>`.
    pub end: f64,
    /// Anchor in world space (Y up).
    pub anchor: Vec3,
    /// Bound values from the event.
    pub bound: BoundValues,
    /// The event's line, in world space.
    pub line: Option<WorldLine>,
    /// Seed of the layers' randomness.
    pub seed: u32,
    /// Time its detail layers were retired, if they were.
    pub detail_retired_at: Option<f64>,
    /// Whether it has detail layers.
    pub has_detail: bool,
    /// Bit `i` set once particle layer `i` has spawned its burst.
    pub bursts_fired: u16,
}

/// A slot: its generation and its instance, if live.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    /// Incremented every time the slot is freed.
    generation: u32,
    /// The live instance.
    instance: Option<EffectInstance>,
}

/// The pool is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the effect instance pool is full ({capacity} slots)")]
pub struct PoolFull {
    /// The pool's capacity.
    pub capacity: u32,
}

/// Fixed-capacity pool of live effects.
// PERFORMANCE RATIONALE:
// Why: A splash stream spawns and ends hundreds of effects per second; an entity or a boxed
// value per effect would allocate in the main world every time (plan D-7).
// What: A slot array and a free list allocated once at the budget's capacity; handles carry a
// generation so a stale handle can never reach a reused slot.
// Trade-off: The capacity is a hard cap chosen in data; an effect beyond it is refused and
// counted instead of growing the pool during a battle.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectInstancePool {
    /// Every slot.
    slots: Vec<Slot>,
    /// Indices of free slots, used last-in first-out.
    free: Vec<u32>,
    /// Live effects per class, by [`EffectClass::index`].
    live_by_class: [u32; 3],
}

impl EffectInstancePool {
    /// A pool of `capacity` slots, all free.
    #[must_use]
    pub fn with_capacity(capacity: u32) -> Self {
        let slots = (0..capacity)
            .map(|_| Slot {
                generation: 0,
                instance: None,
            })
            .collect();
        Self {
            slots,
            free: (0..capacity).rev().collect(),
            live_by_class: [0; 3],
        }
    }

    /// Number of slots.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        u32::try_from(self.slots.len()).unwrap_or(u32::MAX)
    }

    /// Live effects of `class`.
    #[must_use]
    pub const fn live(&self, class: EffectClass) -> u32 {
        self.live_by_class[class.index()]
    }

    /// Live effects in all.
    #[must_use]
    pub fn live_total(&self) -> u32 {
        self.live_by_class.iter().sum()
    }

    /// Places `instance` in a free slot.
    ///
    /// # Errors
    /// [`PoolFull`] when every slot is live.
    pub fn spawn(&mut self, instance: EffectInstance) -> Result<EffectHandle, PoolFull> {
        let capacity = self.capacity();
        let index = self.free.pop().ok_or(PoolFull { capacity })?;
        let slot = self
            .slots
            .get_mut(index as usize)
            .ok_or(PoolFull { capacity })?;
        slot.instance = Some(instance);
        self.live_by_class[instance.class.index()] += 1;
        Ok(EffectHandle {
            index,
            generation: slot.generation,
        })
    }

    /// The live instance `handle` refers to, if still live.
    #[must_use]
    pub fn get(&self, handle: EffectHandle) -> Option<&EffectInstance> {
        let slot = self.slots.get(handle.index as usize)?;
        (slot.generation == handle.generation)
            .then_some(slot.instance.as_ref())
            .flatten()
    }

    /// Frees the slot `handle` refers to; a stale handle does nothing.
    pub fn free(&mut self, handle: EffectHandle) {
        let Some(slot) = self.slots.get_mut(handle.index as usize) else {
            return;
        };
        if slot.generation != handle.generation {
            return;
        }
        if let Some(instance) = slot.instance.take() {
            self.live_by_class[instance.class.index()] -= 1;
            slot.generation = slot.generation.wrapping_add(1);
            self.free.push(handle.index);
        }
    }

    /// Frees every effect whose `end` is before `now`.
    pub fn free_ended(&mut self, now: f64) {
        for index in 0..self.slots.len() {
            let slot = &self.slots[index];
            if let Some(instance) = slot.instance {
                if instance.end < now {
                    let handle = EffectHandle {
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                        generation: slot.generation,
                    };
                    self.free(handle);
                }
            }
        }
    }

    /// Frees every effect.
    pub fn clear(&mut self) {
        for index in 0..self.slots.len() {
            let generation = self.slots[index].generation;
            self.free(EffectHandle {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                generation,
            });
        }
    }

    /// Every live instance with its slot index.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &EffectInstance)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.instance.as_ref().map(|instance| (index, instance)))
    }

    /// The live instance in slot `index`, mutably.
    pub fn slot_mut(&mut self, index: usize) -> Option<&mut EffectInstance> {
        self.slots.get_mut(index)?.instance.as_mut()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a failed test is a panic")]
mod tests {
    use super::*;

    fn instance(class: EffectClass, end: f64) -> EffectInstance {
        EffectInstance {
            effect: EffectId(0),
            class,
            birth: 0.0,
            end,
            anchor: Vec3::ZERO,
            bound: BoundValues::default(),
            line: None,
            seed: 0,
            detail_retired_at: None,
            has_detail: true,
            bursts_fired: 0,
        }
    }

    #[test]
    fn spawning_fills_slots_and_a_full_pool_refuses() {
        let mut pool = EffectInstancePool::with_capacity(2);
        let first = pool
            .spawn(instance(EffectClass::Major, 1.0))
            .expect("free slot");
        pool.spawn(instance(EffectClass::Minor, 1.0))
            .expect("free slot");
        assert_eq!(
            pool.spawn(instance(EffectClass::Minor, 1.0)),
            Err(PoolFull { capacity: 2 })
        );
        assert_eq!(pool.live(EffectClass::Major), 1);
        assert_eq!(pool.live(EffectClass::Minor), 1);
        assert_eq!(pool.live_total(), 2);
        assert!(pool.get(first).is_some());
    }

    #[test]
    fn a_stale_handle_never_reaches_a_reused_slot() {
        let mut pool = EffectInstancePool::with_capacity(1);
        let old = pool
            .spawn(instance(EffectClass::Major, 1.0))
            .expect("free slot");
        pool.free(old);
        assert!(pool.get(old).is_none());
        let new = pool
            .spawn(instance(EffectClass::Ambient, 1.0))
            .expect("reused slot");
        assert_eq!(new.index, old.index);
        assert_ne!(new.generation, old.generation);
        pool.free(old);
        assert!(
            pool.get(new).is_some(),
            "freeing a stale handle does nothing"
        );
        assert_eq!(pool.live(EffectClass::Ambient), 1);
    }

    #[test]
    fn ended_effects_are_freed_and_the_pool_never_grows() {
        let mut pool = EffectInstancePool::with_capacity(3);
        pool.spawn(instance(EffectClass::Minor, 0.5)).expect("free");
        pool.spawn(instance(EffectClass::Minor, 2.0)).expect("free");
        let capacity = (pool.slots.capacity(), pool.free.capacity());
        pool.free_ended(1.0);
        assert_eq!(pool.live(EffectClass::Minor), 1);
        for _ in 0..100 {
            let handle = pool
                .spawn(instance(EffectClass::Ambient, 0.0))
                .expect("free");
            pool.free(handle);
        }
        assert_eq!((pool.slots.capacity(), pool.free.capacity()), capacity);
        pool.clear();
        assert_eq!(pool.live_total(), 0);
        assert_eq!(pool.iter().count(), 0);
    }
}
