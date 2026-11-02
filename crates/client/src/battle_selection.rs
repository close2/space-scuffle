//! Client-side battle selection and control groups.
//!
//! # Responsibility
//! Owns which units the local player has selected. The simulation has no such concept: every
//! [`RenderSnapshot`](sim_types::RenderSnapshot) reports `selected: false`, so the selection lives
//! here and [`sync_live_battle_system`](crate::sync_live_battle_system) applies it to each bubble
//! it publishes.
//!
//! # Invariants
//! - [`BattleSelection::ids`] is sorted and unique, and holds only the local player's units.
//! - A unit of anyone else can be looked at through [`BattleSelection::inspected`] but never joins
//!   [`BattleSelection::ids`], so it cannot receive orders.
//! - Everything here names entities of one session and one owner. Entity IDs restart with each
//!   session, so a new generation or owner clears all of it.
//! - A unit that leaves the snapshot is forgotten everywhere, including control groups.

use bevy::prelude::*;
use sim_types::{EntityId, PlayerId};

use crate::battle_pipeline::commands::SessionGeneration;
use crate::input_bindings::CONTROL_GROUP_COUNT;

// PERFORMANCE RATIONALE:
// Why: Sync asks "is this unit selected?" for every bubble on every frame of a 400-unit battle.
// What: Sorted vectors answer by binary search, and their storage is reused between selections.
// Trade-off: Editing a selection pays O(S) to keep order, for O(N log S) frames with no allocation.
/// The local player's selection and control groups for the current session.
#[derive(Resource, Debug, Default)]
pub struct BattleSelection {
    generation: Option<SessionGeneration>,
    owner: Option<PlayerId>,
    selected: Vec<EntityId>,
    inspected: Option<EntityId>,
    groups: [Vec<EntityId>; CONTROL_GROUP_COUNT],
}

impl BattleSelection {
    /// Clears everything if `generation` is not the session this selection belongs to.
    pub fn bind_generation(&mut self, generation: Option<SessionGeneration>) {
        if self.generation != generation {
            self.generation = generation;
            self.clear_all();
        }
    }

    /// Clears everything if `owner` is not the player this selection belongs to.
    pub fn bind_owner(&mut self, owner: Option<PlayerId>) {
        if self.owner != owner {
            self.owner = owner;
            self.clear_all();
        }
    }

    /// Removes the selection and every control group.
    pub fn clear_all(&mut self) {
        self.clear();
        for group in &mut self.groups {
            group.clear();
        }
    }

    /// Removes the selection and keeps control groups.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.inspected = None;
    }

    /// Selected units of the local player, sorted by ID.
    #[must_use]
    pub fn ids(&self) -> &[EntityId] {
        &self.selected
    }

    /// Unit of another player being looked at, if any.
    #[must_use]
    pub fn inspected(&self) -> Option<EntityId> {
        self.inspected
    }

    /// Whether `id` should draw as selected.
    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.inspected == Some(id) || self.selected.binary_search(&id).is_ok()
    }

    /// Replaces the selection with the local player's unit `id`.
    pub fn select_only(&mut self, id: EntityId) {
        self.clear();
        self.selected.push(id);
    }

    /// Adds the local player's unit `id`, or removes it if it was selected.
    pub fn toggle(&mut self, id: EntityId) {
        self.inspected = None;
        match self.selected.binary_search(&id) {
            Ok(index) => {
                self.selected.remove(index);
            }
            Err(index) => self.selected.insert(index, id),
        }
    }

    /// Replaces the selection with unit `id` of another player, which is only looked at.
    pub fn inspect(&mut self, id: EntityId) {
        self.clear();
        self.inspected = Some(id);
    }

    /// Adds the local player's units `ids` to the selection.
    pub fn extend(&mut self, ids: impl IntoIterator<Item = EntityId>) {
        self.inspected = None;
        self.selected.extend(ids);
        self.selected.sort_unstable();
        self.selected.dedup();
    }

    /// Stores the selection in control group `index`. An empty selection changes nothing.
    ///
    /// An `index` of [`CONTROL_GROUP_COUNT`] or more does nothing.
    pub fn assign_group(&mut self, index: usize) {
        if self.selected.is_empty() {
            return;
        }
        if let Some(group) = self.groups.get_mut(index) {
            group.clone_from(&self.selected);
        }
    }

    /// Replaces the selection with control group `index`. An empty group changes nothing.
    ///
    /// An `index` of [`CONTROL_GROUP_COUNT`] or more does nothing.
    pub fn recall_group(&mut self, index: usize) {
        let Some(group) = self.groups.get(index).filter(|group| !group.is_empty()) else {
            return;
        };
        self.selected.clone_from(group);
        self.inspected = None;
    }

    /// Members of control group `index`, sorted by ID.
    #[must_use]
    pub fn group(&self, index: usize) -> &[EntityId] {
        self.groups.get(index).map_or(&[], Vec::as_slice)
    }

    /// Removes a unit that has left the battle from the selection and every control group.
    pub fn forget(&mut self, id: EntityId) {
        if self.inspected == Some(id) {
            self.inspected = None;
        }
        if let Ok(index) = self.selected.binary_search(&id) {
            self.selected.remove(index);
        }
        for group in &mut self.groups {
            if let Ok(index) = group.binary_search(&id) {
                group.remove(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[u32]) -> Vec<EntityId> {
        values.iter().copied().map(EntityId).collect()
    }

    #[test]
    fn extend_keeps_ids_sorted_and_unique() {
        let mut selection = BattleSelection::default();
        selection.extend(ids(&[9, 3, 9]));
        selection.extend(ids(&[3, 1]));
        assert_eq!(selection.ids(), ids(&[1, 3, 9]));
        assert!(selection.contains(EntityId(3)));
        assert!(!selection.contains(EntityId(2)));
    }

    #[test]
    fn toggle_adds_then_removes_in_order() {
        let mut selection = BattleSelection::default();
        selection.toggle(EntityId(5));
        selection.toggle(EntityId(2));
        assert_eq!(selection.ids(), ids(&[2, 5]));
        selection.toggle(EntityId(5));
        assert_eq!(selection.ids(), ids(&[2]));
    }

    #[test]
    fn inspected_unit_draws_as_selected_but_never_joins_the_orderable_ids() {
        let mut selection = BattleSelection::default();
        selection.extend(ids(&[1, 2]));
        selection.inspect(EntityId(7));
        assert!(selection.contains(EntityId(7)));
        assert_eq!(selection.ids().len(), 0);
        selection.toggle(EntityId(1));
        assert_eq!(selection.inspected(), None);
        assert_eq!(selection.ids(), ids(&[1]));
    }

    #[test]
    fn empty_operations_leave_groups_and_selection_unchanged() {
        let mut selection = BattleSelection::default();
        selection.extend(ids(&[4]));
        selection.assign_group(0);
        selection.clear();
        selection.assign_group(0);
        assert_eq!(selection.group(0), ids(&[4]));
        selection.select_only(EntityId(8));
        selection.recall_group(1);
        assert_eq!(selection.ids(), ids(&[8]));
    }

    #[test]
    fn out_of_range_groups_do_nothing() {
        let mut selection = BattleSelection::default();
        selection.extend(ids(&[4]));
        selection.assign_group(CONTROL_GROUP_COUNT);
        selection.recall_group(CONTROL_GROUP_COUNT);
        assert_eq!(selection.ids(), ids(&[4]));
        assert_eq!(selection.group(CONTROL_GROUP_COUNT).len(), 0);
    }

    #[test]
    fn recall_replaces_the_selection_with_a_copy_of_the_group() {
        let mut selection = BattleSelection::default();
        selection.extend(ids(&[1, 2]));
        selection.assign_group(3);
        selection.select_only(EntityId(9));
        selection.recall_group(3);
        selection.toggle(EntityId(1));
        assert_eq!(selection.ids(), ids(&[2]));
        assert_eq!(selection.group(3), ids(&[1, 2]), "groups do not alias");
    }

    #[test]
    fn forget_removes_a_unit_from_selection_and_every_group() {
        let mut selection = BattleSelection::default();
        selection.extend(ids(&[1, 2]));
        selection.assign_group(0);
        selection.assign_group(9);
        selection.forget(EntityId(1));
        assert_eq!(selection.ids(), ids(&[2]));
        assert_eq!(selection.group(0), ids(&[2]));
        assert_eq!(selection.group(9), ids(&[2]));
        selection.inspect(EntityId(5));
        selection.forget(EntityId(5));
        assert_eq!(selection.inspected(), None);
    }

    #[test]
    fn a_new_session_or_owner_clears_everything() {
        let mut selection = BattleSelection::default();
        selection.bind_generation(Some(SessionGeneration::FIRST));
        selection.bind_owner(Some(PlayerId(0)));
        selection.extend(ids(&[1]));
        selection.assign_group(0);

        selection.bind_generation(Some(SessionGeneration::FIRST));
        selection.bind_owner(Some(PlayerId(0)));
        assert_eq!(selection.ids(), ids(&[1]), "same scope keeps the selection");

        selection.bind_owner(Some(PlayerId(1)));
        assert_eq!(selection.ids().len(), 0);
        assert_eq!(selection.group(0).len(), 0);

        selection.extend(ids(&[2]));
        selection.assign_group(0);
        selection.bind_generation(SessionGeneration::FIRST.next().ok());
        assert_eq!(selection.ids().len(), 0);
        assert_eq!(selection.group(0).len(), 0);
    }
}
