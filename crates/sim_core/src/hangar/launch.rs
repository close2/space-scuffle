//! Launching, docking and the death of hangar members (plan `2026-11-28-carrier-interceptors`
//! D-4, D-7, D-9): the launch channel a launch weapon's release starts, each try that puts the
//! oldest docked member into the world, the dock that takes a returning member back, and the
//! reap hook that kills launched members with their hangar unit.
//!
//! A launch prepares the member's spawn before it pops the record, so a refused spawn leaves the
//! hangar as it was. A dock removes the member's entity without a death: no shatter, no supply
//! change (a member takes no supply, validated by the catalog).

use super::{DockedMember, HangarInvariant, HangarLink, LaunchChannel};
use crate::SimWorld;
use sim_types::{EntityId, HangarDeath, MagazineLaunch, SimScalar};

impl SimWorld {
    /// Starts the launch channel of a launch weapon's release by `attacker` at `target` (D-4):
    /// the hangar's target becomes `target`; a release during a running channel on the same
    /// target starts nothing, and otherwise the channel starts afresh with its first try on this
    /// tick. Deals no damage and records no reveal, snapshot or launch image.
    pub(crate) fn start_launch_channel(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        launch: MagazineLaunch,
    ) {
        let Some(state) = self
            .entities
            .get_mut(&attacker)
            .and_then(|e| e.hangar.as_mut())
        else {
            return;
        };
        let running = state.channel.is_some() && state.target == Some(target);
        state.target = Some(target);
        if running || launch.tries == 0 {
            return;
        }
        state.channel = Some(LaunchChannel {
            tries_left: launch.tries,
            next_try_in: 0,
        });
        self.launch_try(attacker, launch.interval_ticks);
    }

    /// One try of the channel of `hangar` (D-5): launches the oldest docked member if there is
    /// one, then spends the try; the next is `interval` ticks later, and the channel ends with
    /// its last try. A try with nothing docked launches nothing.
    pub(super) fn launch_try(&mut self, hangar: EntityId, interval: u16) {
        let docked = self
            .entities
            .get(&hangar)
            .and_then(|e| e.hangar.as_ref())
            .is_some_and(|state| !state.docked.is_empty());
        if docked {
            if let Err(source) = self.launch_member(hangar) {
                self.report_hangar_invariant(source);
            }
        }
        let Some(state) = self
            .entities
            .get_mut(&hangar)
            .and_then(|e| e.hangar.as_mut())
        else {
            return;
        };
        if let Some(channel) = state.channel.as_mut() {
            channel.tries_left = channel.tries_left.saturating_sub(1);
            channel.next_try_in = interval;
            if channel.tries_left == 0 {
                state.channel = None;
            }
        }
    }

    /// Puts the oldest docked member of `hangar` into the world at the hangar unit's centre,
    /// facing its heading, with the record's vitals and a link home, and directs it at once
    /// (D-7). The spawn is prepared before the record is popped.
    ///
    /// # Errors
    /// [`HangarInvariant::MissingDefinition`] when the hangar unit, its hangar or its member is
    /// missing, and [`HangarInvariant::Launch`] when the spawn is refused; the record then stays
    /// docked.
    fn launch_member(&mut self, hangar: EntityId) -> Result<(), HangarInvariant> {
        let missing = HangarInvariant::MissingDefinition { hangar };
        let home = self.entities.get(&hangar).ok_or(missing.clone())?;
        let spec = self
            .catalog
            .get(home.unit_type_id)
            .ok()
            .and_then(|arch| arch.capabilities.hangar)
            .ok_or(missing.clone())?;
        let record = *home
            .hangar
            .as_ref()
            .and_then(|state| state.docked.first())
            .ok_or(missing.clone())?;
        let (owner, position, heading) = (home.owner, home.position, home.heading);
        let mut prepared = self
            .prepare_spawn(spec.member, owner, position, 0)
            .map_err(|source| HangarInvariant::Launch {
                hangar,
                source: Box::new(source),
            })?;
        let member = &mut prepared.entity;
        member.hp = record.hp.min(member.max_hp);
        member.shields = record.shields;
        member.ticks_since_last_damage = record.ticks_since_last_damage;
        member.heading = heading;
        member.turret_angle = heading;
        member.hangar_link = Some(HangarLink {
            hangar,
            returning: false,
        });
        let state = self
            .entities
            .get_mut(&hangar)
            .and_then(|e| e.hangar.as_mut())
            .ok_or(missing)?;
        state.docked.remove(0);
        state.launched = state.launched.saturating_add(1);
        let id = self.commit_spawn(prepared);
        // Its first order at once (D-7), by the members pass's own rule.
        self.direct_member(hangar, id);
        Ok(())
    }

    /// Takes the returning member `member` back into `hangar` (D-5): its entity leaves the world
    /// without a death, and its life, shields and regeneration clock are docked as the newest
    /// record.
    pub(super) fn dock_member(&mut self, hangar: EntityId, member: EntityId) {
        let Some(unit) = self.entities.remove(&member) else {
            return;
        };
        // As the reap does: a turret tracks no unit that has left the world.
        if !self.tracked_targets.is_empty() {
            let entities = &self.entities;
            self.tracked_targets
                .retain_units(|unit| entities.contains_key(&unit));
        }
        if let Some(state) = self
            .entities
            .get_mut(&hangar)
            .and_then(|e| e.hangar.as_mut())
        {
            state.launched = state.launched.saturating_sub(1);
            state.docked.push(DockedMember {
                hp: unit.hp,
                shields: unit.shields,
                ticks_since_last_damage: unit.ticks_since_last_damage,
            });
        }
    }

    /// The death hook of hangars (D-9), run first in the reap: every living member whose hangar
    /// unit is dead or gone dies on this tick (`HangarDeath::KillMembers`), to be reaped with its
    /// own shatter and credited to no one; every member that died this tick leaves its living
    /// hangar's launched count. Docked records and queued builds go with the hangar state.
    pub(crate) fn reap_hangar_members(&mut self) {
        if !self.catalog.has_hangars() {
            return;
        }
        let mut scratch = std::mem::take(&mut self.hangar_scratch);
        scratch.members.clear();
        scratch.members.extend(
            self.entities
                .iter()
                .filter_map(|(&id, entity)| entity.hangar_link.map(|link| (link.hangar, id))),
        );
        for &(hangar, member) in &scratch.members {
            let home_alive = self
                .entities
                .get(&hangar)
                .is_some_and(|home| home.is_alive() && home.hangar.is_some());
            let member_alive = self
                .entities
                .get(&member)
                .is_some_and(crate::entity::Entity::is_alive);
            if home_alive && !member_alive {
                if let Some(state) = self
                    .entities
                    .get_mut(&hangar)
                    .and_then(|e| e.hangar.as_mut())
                {
                    state.launched = state.launched.saturating_sub(1);
                }
            } else if !home_alive && member_alive {
                self.kill_orphaned_member(hangar, member);
            }
        }
        self.hangar_scratch = scratch;
    }

    /// Applies the dead hangar's `on_death` rule to its living member `member`.
    fn kill_orphaned_member(&mut self, hangar: EntityId, member: EntityId) {
        let rule = self
            .entities
            .get(&hangar)
            .and_then(|home| self.catalog.get(home.unit_type_id).ok())
            .and_then(|arch| arch.capabilities.hangar)
            .map_or(HangarDeath::KillMembers, |spec| spec.on_death);
        match rule {
            HangarDeath::KillMembers => {
                if let Some(unit) = self.entities.get_mut(&member) {
                    unit.hp = SimScalar::ZERO;
                    unit.shields = SimScalar::ZERO;
                }
            }
        }
    }
}
