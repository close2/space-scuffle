//! The rules of a structure's rally slots (plan `2026-11-25-rallies` D-1, D-3, D-4).
//!
//! # Responsibility
//! Reads an archetype's rally ability, its [`RallySpec`] or the default ability when it has
//! none, and answers the three questions the rally code asks of it:
//! - which slot a `SetRally` sets for a target ([`RallyAbility::slot_taking`], §2.2 rule 3);
//! - which slot a new unit uses ([`RallyAbility::slot_used_by`], §2.2 rule 2, B-217);
//! - how many slots and waypoints the routes may hold ([`RallyAbility::slot_count`],
//!   [`RallyAbility::max_waypoints`]).
//!
//! # How it fits
//! `production::rally` admits commands, resolves routes and gives the new unit its orders; it
//! asks this module for every slot decision, so the rules live in one place. The rules read
//! only catalog data and the harvest capability, never an identity (charter §3.3).
//!
//! # Invariants
//! - The default ability (no spec) is one slot of one waypoint, used by every unit and taking
//!   every target: the rule before rally slots, so an archetype built without a spec behaves
//!   as it did.
//! - Slots are scanned in slot order and the first that qualifies wins, so every decision is
//!   deterministic.

use crate::UnitArchetype;
use sim_types::{RallySlotSpec, RallySpec, RallyTarget, RallyUsers};

/// One slot's rules, borrowed from the spec or the default ability's.
#[derive(Clone, Copy, Debug)]
struct SlotRules<'a> {
    /// Which units use the slot.
    used_by: RallyUsers,
    /// Unit slugs that never use it.
    excludes: &'a [String],
    /// Whether a node may be its target.
    on_resources: bool,
    /// Whether the smart command sets it on a point.
    smart_on_ground: bool,
}

impl<'a> SlotRules<'a> {
    /// The rules of an authored slot.
    fn of(slot: &'a RallySlotSpec) -> Self {
        Self {
            used_by: slot.used_by,
            excludes: &slot.excludes,
            on_resources: slot.on_resources,
            smart_on_ground: slot.smart_on_ground,
        }
    }

    /// The default ability's only slot: every unit, every target.
    const DEFAULT: SlotRules<'static> = SlotRules {
        used_by: RallyUsers::All,
        excludes: &[],
        on_resources: true,
        smart_on_ground: true,
    };

    /// Whether the slot may hold `target` (`SetValidators`): a node needs `on_resources`, and
    /// any other target is accepted. For the smart command (`smart`), a point also needs
    /// `smart_on_ground` (`SetOnGround`).
    fn takes(self, target: RallyTarget, smart: bool) -> bool {
        match target {
            RallyTarget::Node(_) => self.on_resources,
            RallyTarget::Point(_) => !smart || self.smart_on_ground,
            RallyTarget::Unit(_) => true,
        }
    }

    /// Whether `product` uses the slot (`UseFilters`, `UseValidators`): a worker is a unit with
    /// the harvest capability.
    fn used_by(self, product: &UnitArchetype) -> bool {
        let worker = product.capabilities.harvester.is_some();
        let filter = match self.used_by {
            RallyUsers::All => true,
            RallyUsers::Workers => worker,
            RallyUsers::NonWorkers => !worker,
        };
        filter && !self.excludes.contains(&product.slug)
    }
}

/// An archetype's rally ability: its authored [`RallySpec`], or the default ability.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RallyAbility<'a> {
    /// The authored spec; `None` is the default ability.
    spec: Option<&'a RallySpec>,
}

impl<'a> RallyAbility<'a> {
    /// The rally ability of `producer`.
    pub(crate) fn of(producer: &'a UnitArchetype) -> Self {
        Self {
            spec: producer.rally.as_ref(),
        }
    }

    /// How many slots the ability has; at least one (catalog validation).
    pub(crate) fn slot_count(self) -> u8 {
        self.spec
            .map_or(1, |spec| u8::try_from(spec.slots.len()).unwrap_or(u8::MAX))
    }

    /// Most waypoints one slot's route holds; at least one (catalog validation).
    pub(crate) fn max_waypoints(self) -> u8 {
        self.spec.map_or(1, |spec| spec.max_waypoints)
    }

    /// The rules of slot `slot`, if the ability has it.
    fn rules(self, slot: u8) -> Option<SlotRules<'a>> {
        match self.spec {
            None => (slot == 0).then_some(SlotRules::DEFAULT),
            Some(spec) => spec.slots.get(usize::from(slot)).map(SlotRules::of),
        }
    }

    /// The slots in order with their rules.
    fn slots(self) -> impl Iterator<Item = (u8, SlotRules<'a>)> {
        (0..self.slot_count()).filter_map(move |slot| self.rules(slot).map(|rules| (slot, rules)))
    }

    /// The slot a `SetRally` naming `slot` sets for `target` (D-3): the named slot when it
    /// exists and takes the target; for the smart command (`None`), the first slot that takes it
    /// from a right click. `None` when no slot does, so this structure does not take the
    /// command.
    pub(crate) fn slot_taking(self, slot: Option<u8>, target: RallyTarget) -> Option<u8> {
        match slot {
            Some(slot) => self
                .rules(slot)
                .filter(|rules| rules.takes(target, false))
                .map(|_| slot),
            None => self
                .slots()
                .find(|(_, rules)| rules.takes(target, true))
                .map(|(slot, _)| slot),
        }
    }

    /// The slot a new `product` uses (D-4): the first whose use rule accepts it. `None` means
    /// it uses no rally; an empty chosen slot gives no order (no fall-through, B-217).
    pub(crate) fn slot_used_by(self, product: &UnitArchetype) -> Option<u8> {
        self.slots()
            .find(|(_, rules)| rules.used_by(product))
            .map(|(slot, _)| slot)
    }
}

#[cfg(test)]
mod tests {
    use super::{RallyAbility, SlotRules};
    use sim_types::{EntityId, SimVec2};
    use sim_types::{RallySlotSpec, RallySpec, RallyTarget, RallyUsers, ResourceNodeId};

    fn slot(used_by: RallyUsers, on_resources: bool, smart_on_ground: bool) -> RallySlotSpec {
        RallySlotSpec {
            label: "x".to_owned(),
            used_by,
            excludes: vec!["queen".to_owned()],
            on_resources,
            smart_on_ground,
        }
    }

    #[test]
    fn the_default_ability_is_one_slot_that_takes_everything() {
        let ability = RallyAbility { spec: None };
        assert_eq!((ability.slot_count(), ability.max_waypoints()), (1, 1));
        for target in [
            RallyTarget::Point(SimVec2::ZERO),
            RallyTarget::Unit(EntityId(3)),
            RallyTarget::Node(ResourceNodeId(1)),
        ] {
            assert_eq!(ability.slot_taking(None, target), Some(0));
            assert_eq!(ability.slot_taking(Some(0), target), Some(0));
            assert_eq!(ability.slot_taking(Some(1), target), None);
        }
    }

    /// §2.2 rule 3 on the two-slot ability of `RallyHatchery`: a right click on a node sets the
    /// worker slot, one on the ground or a unit the army slot; the worker entry sets its slot
    /// on the ground, and the army entry refuses a node.
    #[test]
    fn the_smart_command_sets_the_first_slot_that_takes_the_target() {
        let spec = RallySpec {
            max_waypoints: 4,
            slots: vec![
                slot(RallyUsers::NonWorkers, false, true),
                slot(RallyUsers::Workers, true, false),
            ],
        };
        let ability = RallyAbility { spec: Some(&spec) };
        let (ground, node, unit) = (
            RallyTarget::Point(SimVec2::ZERO),
            RallyTarget::Node(ResourceNodeId(1)),
            RallyTarget::Unit(EntityId(3)),
        );
        assert_eq!(ability.slot_taking(None, node), Some(1));
        assert_eq!(ability.slot_taking(None, ground), Some(0));
        assert_eq!(ability.slot_taking(None, unit), Some(0));
        assert_eq!(ability.slot_taking(Some(1), ground), Some(1));
        assert_eq!(ability.slot_taking(Some(0), node), None);
        assert_eq!(ability.slot_taking(Some(2), ground), None);
        assert!(!SlotRules::of(&spec.slots[1]).takes(ground, true));
    }
}
