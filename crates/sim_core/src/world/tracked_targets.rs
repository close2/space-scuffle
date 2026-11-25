//! # The Enemies Tracking Turrets Follow
//!
//! Owns the world-level table of the enemy each tracking turret aims at this tick (plan
//! `2026-11-26-turret-tracking-weapon` D-2 as amended by Amendment 1): 5.0.14's hidden
//! `90mmCannonsFake`, which turns the unsieged Siege Tank's turret towards an enemy in reach
//! under any order.
//!
//! ## How it fits
//! Target acquisition writes the table (`SimWorld::acquire_targets`); the combat phase reads it
//! only to aim a turret its real weapon leaves free (`combat::TrackingAim`). An order that
//! replaces a unit's orders, boarding a transport and the reap clear a unit's entry; the
//! checksum hashes the table in unit id order.
//!
//! ## Invariants Maintained
//! 1. Entries are sorted by the tracking unit's id, one per unit at most, so iteration, lookup
//!    and hashing are deterministic.
//! 2. Only units carrying a weapon whose turret tracks any enemy ever hold an entry: acquisition
//!    sets none for any other unit.
//! 3. No float and no hash map: the table is a sorted vector of entity ids.

use sim_types::EntityId;

// PERFORMANCE RATIONALE:
// Why: Storing the tracked enemy on `Entity` grew every entity by eight bytes, which changed the
// code generated for acquisition's candidate loop and cost 2.3 % of the throughput benchmark's
// instructions for a rule only turreted tanks use (plan Amendment 1).
// What: One sorted vector beside the entities, holding entries only for tracking units, looked
// up by binary search and skipped entirely while empty.
// Trade-off: A second place to clear on the events that clear a unit's orders, and an O(log N)
// lookup instead of a field read for the few units that track.
/// The enemy each tracking turret follows, `(unit, enemy)` pairs sorted by unit id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackedTargets {
    entries: Vec<(EntityId, EntityId)>,
}

impl TrackedTargets {
    /// The enemy `unit`'s turret tracks, if any.
    #[must_use]
    pub fn get(&self, unit: EntityId) -> Option<EntityId> {
        if self.entries.is_empty() {
            return None;
        }
        self.entries
            .binary_search_by_key(&unit, |entry| entry.0)
            .ok()
            .map(|index| self.entries[index].1)
    }

    /// Makes `unit`'s turret track `enemy`, replacing any enemy it tracked.
    pub fn set(&mut self, unit: EntityId, enemy: EntityId) {
        match self.entries.binary_search_by_key(&unit, |entry| entry.0) {
            Ok(index) => self.entries[index].1 = enemy,
            Err(index) => self.entries.insert(index, (unit, enemy)),
        }
    }

    /// Forgets the enemy `unit`'s turret tracks; nothing happens if it tracks none.
    pub fn release(&mut self, unit: EntityId) {
        if let Ok(index) = self.entries.binary_search_by_key(&unit, |entry| entry.0) {
            self.entries.remove(index);
        }
    }

    /// Keeps only the entries of units `keep` accepts, in order.
    pub fn retain_units(&mut self, mut keep: impl FnMut(EntityId) -> bool) {
        self.entries.retain(|entry| keep(entry.0));
    }

    /// Whether no turret tracks anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The `(unit, enemy)` pairs in unit id order.
    #[must_use]
    pub fn entries(&self) -> &[(EntityId, EntityId)] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::TrackedTargets;
    use sim_types::EntityId;

    #[test]
    fn entries_stay_sorted_unique_and_releasable() {
        let mut table = TrackedTargets::default();
        assert_eq!(table.get(EntityId(4)), None);
        table.set(EntityId(9), EntityId(1));
        table.set(EntityId(4), EntityId(2));
        table.set(EntityId(9), EntityId(3));
        assert_eq!(
            table.entries(),
            &[(EntityId(4), EntityId(2)), (EntityId(9), EntityId(3))]
        );
        assert_eq!(table.get(EntityId(9)), Some(EntityId(3)));
        table.release(EntityId(4));
        table.release(EntityId(5));
        assert_eq!(table.entries(), &[(EntityId(9), EntityId(3))]);
        table.retain_units(|unit| unit != EntityId(9));
        assert!(table.is_empty());
    }
}
