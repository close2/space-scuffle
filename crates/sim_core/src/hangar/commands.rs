//! The hangar's player commands (plan `2026-12-01-death-spawns-and-hangar-commands` D-6, D-7):
//! build one member, cancel the newest build for a full refund, and switch automatic builds.
//!
//! # Responsibility
//! Admits `UnitOrder::HangarBuild`, `UnitOrder::HangarCancelLast` and
//! `UnitOrder::SetHangarAutoBuild` for a selection and applies them to every selected hangar
//! unit, answering with a [`GameCommandReceipt`] or a typed [`HangarRejection`].
//!
//! # How it fits
//! The commands are instant game commands: `route_command` sends them to the economy's
//! `admit_game_command`, which calls [`SimWorld::admit_hangar_command`] after the usual checks of
//! the match state and of housed and unselectable recipients. They never enter an order queue,
//! cancel no order, and ignore Shift, as `CancelProduction` does. A build shares its queueing
//! rule and debit with the hangar phase's automatic builds through
//! [`try_queue_build`](super::try_queue_build).
//!
//! # Invariants
//! - Recipients are visited in ascending entity id, each once, so a command acts identically
//!   on every peer whatever order its selection lists.
//! - A command changes nothing unless at least one hangar unit takes it; then the receipt counts
//!   the hangar units that took it, and a refusal names the first recipient's reason in id order.
//! - A cancel refunds exactly what its build was charged: the member's current cost, which is
//!   what the build paid (catalog costs do not change during a match).

use super::try_queue_build;
use crate::economy::{GameCommandError, GameCommandReceipt};
use crate::SimWorld;
use sim_types::{EntityId, HangarSpec, PlayerId, UnitOrder};
use thiserror::Error;

/// Why a hangar command changed nothing (plan `2026-12-01-death-spawns-and-hangar-commands`
/// D-6), wrapped by [`GameCommandError::Hangar`]. For a selection of several hangar units it is
/// the reason of the first in entity id order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HangarRejection {
    /// Nothing selected has a hangar.
    #[error("no selected unit has a hangar")]
    NoHangar,
    /// Docked, launched and queued members already reach the hangar's `max_count`.
    #[error("the hangar holds its most members")]
    Full,
    /// The hangar's build queue holds its `queue_size` builds.
    #[error("the hangar's build queue is full")]
    QueueFull,
    /// The owner has too few minerals for one member.
    #[error("not enough minerals")]
    Minerals,
    /// The owner has too little gas for one member.
    #[error("not enough gas")]
    Gas,
    /// The hangar has no build queued to cancel.
    #[error("the hangar has no build queued")]
    NothingQueued,
    /// The hangar's automatic builds cannot be switched (`HangarSpec::auto_build_toggle`).
    #[error("the hangar's automatic builds cannot be switched")]
    NoAutoBuildSwitch,
}

/// One of the three hangar commands, decoded once from its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HangarCommand {
    /// `UnitOrder::HangarBuild`.
    Build,
    /// `UnitOrder::HangarCancelLast`.
    CancelLast,
    /// `UnitOrder::SetHangarAutoBuild`.
    SetAutoBuild(bool),
}

impl HangarCommand {
    /// The hangar command `order` carries, if it is one.
    const fn of(order: &UnitOrder) -> Option<Self> {
        match *order {
            UnitOrder::HangarBuild => Some(Self::Build),
            UnitOrder::HangarCancelLast => Some(Self::CancelLast),
            UnitOrder::SetHangarAutoBuild { enabled } => Some(Self::SetAutoBuild(enabled)),
            _ => None,
        }
    }
}

impl SimWorld {
    /// Admits a hangar command of `player` for `recipients` and applies it to each selected
    /// hangar unit in ascending id (plan `2026-12-01-death-spawns-and-hangar-commands` D-6).
    /// Recipients without a hangar are skipped.
    ///
    /// # Errors
    /// The selection's own refusals (`InvalidPlayer`, `EmptySelection`, `MissingRecipient`,
    /// `ForeignRecipient`, `DeadRecipient`, `LoadedRecipient`); `UnsupportedOrder` for an order
    /// that is no hangar command; [`HangarRejection::NoHangar`] when no recipient has a hangar;
    /// otherwise, when no hangar unit took the command, the first one's [`HangarRejection`].
    // PERFORMANCE RATIONALE:
    // Why: Recipients must act in entity id order, each once, but the selection is borrowed from
    //      the tick bundle in the order the player listed it.
    // What: The ids are copied into one sorted, deduplicated list per command.
    // Trade-off: One small allocation per hangar command, a player action and never a per-tick
    //            path, instead of a quadratic scan of the selection for each next id.
    pub(crate) fn admit_hangar_command(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        order: &UnitOrder,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        let command = HangarCommand::of(order).ok_or(GameCommandError::UnsupportedOrder)?;
        self.validate_selection(player, recipients)?;
        let mut ids = recipients.to_vec();
        ids.sort_unstable();
        ids.dedup();
        let mut taken = 0_usize;
        let mut first_refusal = None;
        for id in ids {
            let Some(spec) = self.hangar_spec_of(id) else {
                continue;
            };
            match self.apply_hangar_command(id, &spec, command) {
                Ok(()) => taken += 1,
                Err(reason) => {
                    first_refusal.get_or_insert(reason);
                }
            }
        }
        if taken > 0 {
            return Ok(GameCommandReceipt::Accepted { recipients: taken });
        }
        Err(GameCommandError::Hangar(
            first_refusal.unwrap_or(HangarRejection::NoHangar),
        ))
    }

    /// The hangar of the completed unit `id`, if it has hangar state and its archetype a hangar.
    fn hangar_spec_of(&self, id: EntityId) -> Option<HangarSpec> {
        let entity = self.entities.get(&id)?;
        if entity.hangar.is_none() || !entity.is_completed {
            return None;
        }
        self.catalog.get(entity.unit_type_id).ok()?.capabilities.hangar
    }

    /// Applies `command` to the hangar unit `id` of hangar `spec`, or says why it cannot.
    fn apply_hangar_command(
        &mut self,
        id: EntityId,
        spec: &HangarSpec,
        command: HangarCommand,
    ) -> Result<(), HangarRejection> {
        let cost = self
            .catalog
            .get(spec.member)
            .map(|member| (member.mineral_cost, member.gas_cost))
            .map_err(|_| HangarRejection::NoHangar)?;
        let Some(entity) = self.entities.get_mut(&id) else {
            return Err(HangarRejection::NoHangar);
        };
        let Some(slot) = entity.owner.slot() else {
            return Err(HangarRejection::NoHangar);
        };
        let Some(state) = entity.hangar.as_mut() else {
            return Err(HangarRejection::NoHangar);
        };
        let resources = &mut self.player_resources[slot];
        match command {
            HangarCommand::Build => try_queue_build(state, spec, resources, cost),
            HangarCommand::CancelLast => {
                if state.queued == 0 {
                    return Err(HangarRejection::NothingQueued);
                }
                state.queued -= 1;
                if state.queued == 0 {
                    state.build_progress = 0;
                }
                resources.minerals = resources.minerals.saturating_add(cost.0);
                resources.gas = resources.gas.saturating_add(cost.1);
                Ok(())
            }
            HangarCommand::SetAutoBuild(enabled) => {
                if !spec.auto_build_toggle {
                    return Err(HangarRejection::NoAutoBuildSwitch);
                }
                state.auto_build = enabled;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{HangarCommand, HangarRejection};
    use crate::economy::{GameCommandError, GameCommandReceipt};
    use crate::hangar::{try_queue_build, HangarState};
    use crate::world::PlayerResources;
    use crate::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        EntityId, HangarDeath, HangarSpec, PlayerId, SimScalar, SimVec2, UnitOrder, UnitTypeId,
    };

    fn spec() -> HangarSpec {
        HangarSpec {
            member: UnitTypeId(1),
            max_count: 8,
            start_count: 4,
            build_ticks: 214,
            queue_size: 5,
            auto_build: true,
            auto_build_toggle: true,
            leash: SimScalar::from_i32(12),
            on_death: HangarDeath::KillMembers,
        }
    }

    fn rich() -> PlayerResources {
        PlayerResources {
            minerals: 1000,
            gas: 1000,
            ..PlayerResources::default()
        }
    }

    /// D-6: a build debits the cost and queues one; the bounds refuse in the order full, queue
    /// full, minerals, gas, and a refusal changes nothing.
    #[test]
    fn try_queue_build_checks_room_then_money_and_changes_nothing_on_refusal() {
        let spec = spec();
        let mut state = HangarState::new(&spec);
        let mut resources = rich();
        assert_eq!(
            try_queue_build(&mut state, &spec, &mut resources, (15, 2)),
            Ok(())
        );
        assert_eq!((state.queued, resources.minerals, resources.gas), (1, 985, 998));
        for _ in 0..4 {
            try_queue_build(&mut state, &spec, &mut resources, (15, 0)).expect("room");
        }
        assert_eq!(
            try_queue_build(&mut state, &spec, &mut resources, (15, 0)),
            Err(HangarRejection::QueueFull)
        );
        state.launched = 3;
        assert_eq!(
            try_queue_build(&mut state, &spec, &mut resources, (15, 0)),
            Err(HangarRejection::Full),
            "full is named before a full queue"
        );
        let mut empty = HangarState::new(&spec);
        let mut poor = PlayerResources {
            minerals: 14,
            gas: 0,
            ..PlayerResources::default()
        };
        assert_eq!(
            try_queue_build(&mut empty, &spec, &mut poor, (15, 0)),
            Err(HangarRejection::Minerals)
        );
        poor.minerals = 15;
        assert_eq!(
            try_queue_build(&mut empty, &spec, &mut poor, (15, 1)),
            Err(HangarRejection::Gas)
        );
        assert_eq!((empty.queued, poor.minerals, poor.gas), (0, 15, 0));
    }

    /// D-7: a new hangar's switch is its spec's `auto_build`.
    #[test]
    fn a_new_hangar_takes_its_switch_from_the_spec() {
        assert!(HangarState::new(&spec()).auto_build);
        let off = HangarSpec {
            auto_build: false,
            ..spec()
        };
        assert!(!HangarState::new(&off).auto_build);
    }

    /// D-6: only the three hangar orders decode to a hangar command.
    #[test]
    fn only_the_hangar_orders_are_hangar_commands() {
        assert_eq!(
            HangarCommand::of(&UnitOrder::HangarBuild),
            Some(HangarCommand::Build)
        );
        assert_eq!(
            HangarCommand::of(&UnitOrder::HangarCancelLast),
            Some(HangarCommand::CancelLast)
        );
        assert_eq!(
            HangarCommand::of(&UnitOrder::SetHangarAutoBuild { enabled: true }),
            Some(HangarCommand::SetAutoBuild(true))
        );
        assert_eq!(HangarCommand::of(&UnitOrder::Stop), None);
        assert_eq!(HangarCommand::of(&UnitOrder::UnloadAll), None);
    }

    /// D-6: a selection listed out of order and with a repeated id acts on each hangar once,
    /// and a non-hangar order is not admitted here.
    #[test]
    fn recipients_act_once_each_whatever_the_listed_order() {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        let mut world: SimWorld = crate::test_support::world(map, 0x0B_A7);
        let host = world
            .catalog
            .archetypes()
            .find(|arch| arch.capabilities.hangar.is_some())
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a hangar unit");
        let a = world
            .spawn_unit(host, PlayerId(0), SimVec2::from_i32(8, 16))
            .expect("open ground");
        let b = world
            .spawn_unit(host, PlayerId(0), SimVec2::from_i32(20, 16))
            .expect("open ground");
        let slot = PlayerId(0).slot().expect("slot");
        world.player_resources[slot].minerals = 1000;
        let receipt = world.admit_hangar_command(PlayerId(0), &[b, a, b], &UnitOrder::HangarBuild);
        assert_eq!(receipt, Ok(GameCommandReceipt::Accepted { recipients: 2 }));
        for id in [a, b] {
            assert_eq!(world.entities[&id].hangar().expect("hangar").queued, 1);
        }
        assert_eq!(
            world.admit_hangar_command(PlayerId(0), &[a], &UnitOrder::Stop),
            Err(GameCommandError::UnsupportedOrder)
        );
        assert_eq!(
            world.admit_hangar_command(PlayerId(0), &[EntityId(999)], &UnitOrder::HangarBuild),
            Err(GameCommandError::MissingRecipient(EntityId(999)))
        );
    }
}
