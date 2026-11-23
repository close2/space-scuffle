//! A set of resource kinds: the kinds a harvester acquires on its own.
//!
//! # Responsibility
//! [`ResourceKindSet`] holds a subset of [`ResourceKind`], one bit per kind. A worker's
//! [`HarvesterSpec`](crate::HarvesterSpec) uses it for the harvest ability's `ResourceAcquire`
//! (`StarCraft` II 5.0.14, core `AbilData` 368): the kinds for which the worker looks for another
//! node by itself, when its node is occupied or exhausted (plan `2026-11-23-harvest-occupancy`
//! D-4).
//!
//! # How it fits
//! `sim_core`'s catalog reads the authored list (`acquires: [Mineral]`) into a set at load and
//! refuses a list that names a kind twice; the set itself is a plain value, so repeated
//! insertions count once. Nothing here is hashed.
//!
//! # Invariants
//! - Every kind has its own bit, and no other bit is ever set.
//! - The set is `Copy`, `Eq` and `Hash`, so the capability bundle that holds it stays so.

use crate::ResourceKind;
use serde::{Deserialize, Serialize};

/// A subset of the resource kinds, one bit per kind.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceKindSet {
    bits: u8,
}

impl ResourceKindSet {
    /// The set holding no kind.
    pub const EMPTY: Self = Self { bits: 0 };

    /// The bit of `kind`.
    const fn bit(kind: ResourceKind) -> u8 {
        match kind {
            ResourceKind::Mineral => 1 << 0,
            ResourceKind::Gas => 1 << 1,
        }
    }

    /// The set holding exactly the kinds of `kinds`; a repeated kind counts once.
    #[must_use]
    pub fn of(kinds: &[ResourceKind]) -> Self {
        kinds.iter().fold(Self::EMPTY, |set, &kind| set.with(kind))
    }

    /// This set with `kind` added.
    #[must_use]
    pub const fn with(self, kind: ResourceKind) -> Self {
        Self {
            bits: self.bits | Self::bit(kind),
        }
    }

    /// Adds `kind` to the set.
    pub fn insert(&mut self, kind: ResourceKind) {
        *self = self.with(kind);
    }

    /// Whether the set holds `kind`.
    #[must_use]
    pub const fn contains(self, kind: ResourceKind) -> bool {
        self.bits & Self::bit(kind) != 0
    }

    /// Whether the set holds no kind.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [ResourceKind; 2] = [ResourceKind::Mineral, ResourceKind::Gas];

    #[test]
    fn the_empty_set_holds_nothing() {
        assert!(ResourceKindSet::EMPTY.is_empty());
        assert_eq!(ResourceKindSet::default(), ResourceKindSet::EMPTY);
        for kind in KINDS {
            assert!(!ResourceKindSet::EMPTY.contains(kind));
        }
    }

    #[test]
    fn an_inserted_kind_is_contained_and_no_other() {
        for kind in KINDS {
            let mut set = ResourceKindSet::EMPTY;
            set.insert(kind);
            assert!(set.contains(kind));
            assert!(!set.is_empty());
            for other in KINDS.into_iter().filter(|&other| other != kind) {
                assert!(!set.contains(other), "{kind:?} set holds {other:?}");
            }
        }
    }

    #[test]
    fn every_kind_has_its_own_bit() {
        let both = ResourceKindSet::of(&KINDS);
        assert!(KINDS.into_iter().all(|kind| both.contains(kind)));
        assert_ne!(
            ResourceKindSet::of(&[ResourceKind::Mineral]),
            ResourceKindSet::of(&[ResourceKind::Gas])
        );
    }

    #[test]
    fn a_list_becomes_a_set_and_a_repeat_counts_once() {
        let once = ResourceKindSet::of(&[ResourceKind::Mineral]);
        let twice = ResourceKindSet::of(&[ResourceKind::Mineral, ResourceKind::Mineral]);
        assert_eq!(
            once, twice,
            "the set does not refuse a repeat; the catalog does"
        );
        assert_eq!(ResourceKindSet::of(&[]), ResourceKindSet::EMPTY);
        let mut inserted = once;
        inserted.insert(ResourceKind::Mineral);
        assert_eq!(inserted, once);
    }
}
