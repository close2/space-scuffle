//! The hangar phase (plan `2026-11-28-carrier-interceptors` D-5, D-6): the members pass, which
//! directs every launched member through the existing order types and docks those that are home,
//! and the hangars pass, which checks each hangar, ends lost targets, runs the launch channel,
//! regenerates docked records, builds members and queues new builds.
//!
//! Both passes visit entities in ascending id. The id lists live in [`HangarScratch`], which
//! keeps its capacity between ticks, so the phase allocates nothing in steady state.

use super::{try_queue_build, DockedMember, HangarInvariant, HangarLink};
use crate::catalog::UnitArchetype;
use crate::movement::tuning::ARRIVAL_TOUCH_MARGIN;
use crate::movement::MovementInvariant;
use crate::weapon_timing::SimulationDiagnostic;
use crate::SimWorld;
use sim_types::{
    AttackOrigin, EntityId, FollowKind, HangarSpec, MagazineLaunch, PlayerId, SimScalar, Tick,
    UnitOrder,
};

/// The reusable id lists of the hangar phase and its death hook, not hashed: they are rebuilt
/// from the entity map before every read.
// PERFORMANCE RATIONALE:
// Why: The phase must mutate members and hangar units while it walks them, and it runs every
//      tick a hangar archetype exists.
// What: Two id lists, cleared and refilled each tick, whose capacity survives between ticks.
// Trade-off: High-water memory of one id pair per launched member and one id per hangar unit,
//            instead of an allocation per tick or a clone of the entity map.
#[derive(Debug, Clone, Default)]
pub(crate) struct HangarScratch {
    /// `(hangar, member)` for every living launched member, in member order, then sorted.
    pub(super) members: Vec<(EntityId, EntityId)>,
    /// Every living hangar unit, ascending.
    hangars: Vec<EntityId>,
}

/// What the hangars pass reads of one hangar unit's archetype and owner, resolved once per
/// hangar per tick.
#[derive(Debug, Clone, Copy)]
pub(super) struct HangarRules {
    /// The hangar.
    pub(super) spec: HangarSpec,
    /// The unit's launch weapon's launch, if it has one.
    pub(super) launch: Option<MagazineLaunch>,
    /// A freshly built member's record: the member's maxima for the owner, the clock at rest.
    pub(super) full: DockedMember,
    /// The member's cost in minerals and gas.
    pub(super) cost: (u32, u32),
}

impl SimWorld {
    /// The hangar phase: directs and docks launched members, then steps every hangar unit (plan
    /// `2026-11-28-carrier-interceptors` D-5). Skipped when the catalog has no hangar archetype.
    /// Runs after vision and before acquisition.
    pub fn step_hangars(&mut self) {
        if !self.catalog.has_hangars() {
            return;
        }
        let mut scratch = std::mem::take(&mut self.hangar_scratch);
        scratch.members.clear();
        // Every linked member, dead ones too: a member killed earlier this tick still counts
        // until the reap takes it out of its hangar's `launched`.
        scratch.members.extend(
            self.entities
                .iter()
                .filter_map(|(&id, entity)| entity.hangar_link.map(|link| (link.hangar, id))),
        );
        for &(hangar, member) in &scratch.members {
            if self
                .entities
                .get(&member)
                .is_some_and(crate::entity::Entity::is_alive)
            {
                self.direct_member(hangar, member);
            }
        }
        scratch.members.sort_unstable();
        scratch.hangars.clear();
        scratch.hangars.extend(
            self.entities
                .iter()
                .filter(|(_, entity)| entity.hangar.is_some() && entity.is_alive())
                .map(|(&id, _)| id),
        );
        for &hangar in &scratch.hangars {
            let start = scratch.members.partition_point(|&(h, _)| h < hangar);
            let end = scratch.members.partition_point(|&(h, _)| h <= hangar);
            let counted = scratch.members[start..end]
                .iter()
                .filter(|(_, member)| self.entities.contains_key(member))
                .count();
            self.step_hangar_unit(hangar, counted);
        }
        self.hangar_scratch = scratch;
    }

    /// Records a hangar invariant failure for this tick.
    pub(super) fn report_hangar_invariant(&mut self, source: HangarInvariant) {
        self.tick_diagnostics
            .push(SimulationDiagnostic::HangarInvariant {
                tick: Tick(self.current_tick),
                source,
            });
    }

    /// Directs the launched member `member` of `hangar` (D-6) and docks it when it is home: beyond
    /// the leash it returns; inside it, it attacks the hangar's target while that is a valid
    /// target for it, and returns otherwise. A member whose hangar unit is dead is left to the
    /// death hook.
    pub(super) fn direct_member(&mut self, hangar: EntityId, member: EntityId) {
        let Some(home) = self.entities.get(&hangar) else {
            self.report_hangar_invariant(HangarInvariant::OrphanLink { member, hangar });
            return;
        };
        let Some(state) = home.hangar.as_ref() else {
            self.report_hangar_invariant(HangarInvariant::OrphanLink { member, hangar });
            return;
        };
        if !home.is_alive() {
            return;
        }
        let (home_owner, home_position, home_type, target) =
            (home.owner, home.position, home.unit_type_id, state.target);
        let Some(unit) = self.entities.get(&member) else {
            return;
        };
        if unit.owner != home_owner {
            self.report_hangar_invariant(HangarInvariant::CrossOwner {
                member,
                member_owner: unit.owner,
                hangar,
            });
            return;
        }
        let (Ok(home_arch), Ok(member_arch)) = (
            self.catalog.get(home_type),
            self.catalog.get(unit.unit_type_id),
        ) else {
            self.report_hangar_invariant(HangarInvariant::MissingDefinition { hangar });
            return;
        };
        let Some(spec) = home_arch.capabilities.hangar else {
            self.report_hangar_invariant(HangarInvariant::MissingDefinition { hangar });
            return;
        };
        let centre = unit.position.distance(home_position);
        let touch =
            home_arch.collision_radius + member_arch.collision_radius + ARRIVAL_TOUCH_MARGIN;
        let attack = target.filter(|&t| centre <= spec.leash && self.may_attack(member, t));
        let order = attack.map_or(
            UnitOrder::Follow {
                target: hangar,
                kind: FollowKind::Move,
            },
            |target| UnitOrder::AttackTarget {
                target,
                origin: AttackOrigin::Explicit,
            },
        );
        let returning = attack.is_none();
        self.give_member_order(member, hangar, returning, order);
        if returning && centre <= touch {
            self.dock_member(hangar, member);
        }
    }

    /// Writes the member's link and, when it differs from the member's active order, its order
    /// (D-6): an unchanged row and target leaves the order, so its route and wind-up survive.
    fn give_member_order(
        &mut self,
        member: EntityId,
        hangar: EntityId,
        returning: bool,
        order: UnitOrder,
    ) {
        let Some(unit) = self.entities.get_mut(&member) else {
            return;
        };
        unit.hangar_link = Some(HangarLink { hangar, returning });
        if holds_order(unit.order_queue.front(), &order) {
            return;
        }
        if let Err(source) = unit.issue_order(order, false) {
            self.tick_diagnostics
                .push(SimulationDiagnostic::MovementInvariant {
                    tick: Tick(self.current_tick),
                    source: MovementInvariant::Admission {
                        entity: member,
                        source,
                    },
                });
        }
    }

    /// Whether `attacker` may attack `target` now: the target is alive, neither carried nor
    /// housed, passes `attacker`'s owner's weapon filters (seen, detected if concealed), and the
    /// attacker's weapon admits its plane and attributes, the rule a player's attack order is
    /// admitted by (D-6).
    pub(super) fn may_attack(&self, attacker: EntityId, target: EntityId) -> bool {
        let (Some(unit), Some(victim)) = (self.entities.get(&attacker), self.entities.get(&target))
        else {
            return false;
        };
        if !victim.is_alive() || victim.is_loaded_in_transport() || victim.is_housed() {
            return false;
        }
        let (Ok(arch), Ok(victim_arch)) = (
            self.catalog.get(unit.unit_type_id),
            self.catalog.get(victim.unit_type_id),
        ) else {
            return false;
        };
        self.is_weapon_target_available(unit.owner, target)
            && unit.can_attack(arch, victim, victim_arch)
    }

    /// Steps the living hangar unit `id`, `counted` of whose members are in the world (D-5 step
    /// 2): checks it, ends a lost target, runs the channel, regenerates docked records, builds
    /// and queues. A failed check reports and leaves the hangar untouched.
    fn step_hangar_unit(&mut self, id: EntityId, counted: usize) {
        let Some(rules) = self.hangar_rules(id) else {
            self.report_hangar_invariant(HangarInvariant::MissingDefinition { hangar: id });
            return;
        };
        if let Err(source) = self.check_hangar(id, counted, &rules) {
            self.report_hangar_invariant(source);
            return;
        }
        self.check_hangar_target(id);
        self.advance_channel(id, rules.launch);
        let (owner, member_type) = match self.entities.get(&id) {
            Some(unit) => (unit.owner, rules.spec.member),
            None => return,
        };
        let Ok(member_arch) = self.catalog.get(member_type) else {
            return;
        };
        let full = rules.full;
        let Some(state) = self.entities.get_mut(&id).and_then(|e| e.hangar.as_mut()) else {
            return;
        };
        for record in &mut state.docked {
            crate::world::upkeep::regenerate_docked(member_arch, full.hp, record);
        }
        if state.queued > 0 {
            state.build_progress = state.build_progress.saturating_add(1);
            if state.build_progress >= rules.spec.build_ticks {
                state.docked.push(full);
                state.queued -= 1;
                state.build_progress = 0;
            }
        }
        self.auto_build(id, owner, &rules);
    }

    /// The archetype facts the hangars pass reads for the hangar unit `id`; `None` when its
    /// archetype, hangar or member is missing.
    fn hangar_rules(&self, id: EntityId) -> Option<HangarRules> {
        let unit = self.entities.get(&id)?;
        let arch = self.catalog.get(unit.unit_type_id).ok()?;
        let spec = arch.capabilities.hangar?;
        let member = self.catalog.get(spec.member).ok()?;
        Some(HangarRules {
            spec,
            launch: arch.launch_weapon(),
            full: self.full_member(unit.owner, member),
            cost: (member.mineral_cost, member.gas_cost),
        })
    }

    /// A member of archetype `member` at its full life and shields for `owner` (its upgrades'
    /// life bonus included, as a spawn applies it), its regeneration clock at rest.
    pub(crate) fn full_member(&self, owner: PlayerId, member: &UnitArchetype) -> DockedMember {
        let delta = owner
            .slot()
            .and_then(|slot| {
                self.player_upgrades[slot]
                    .unit_stat_modifiers
                    .get(&member.slug)
                    .map(|modifiers| modifiers.max_hp_delta)
            })
            .unwrap_or(SimScalar::ZERO);
        DockedMember {
            hp: (member.max_hp + delta).max(SimScalar::from_raw(1)),
            shields: member.max_shields,
            ticks_since_last_damage: u16::MAX,
        }
    }

    /// The D-8 checks of one hangar: its launched count against the members counted, its total
    /// against `max_count`, and its docked records against the member's maxima.
    fn check_hangar(
        &self,
        id: EntityId,
        counted: usize,
        rules: &HangarRules,
    ) -> Result<(), HangarInvariant> {
        let Some(state) = self.entities.get(&id).and_then(|e| e.hangar.as_ref()) else {
            return Err(HangarInvariant::MissingDefinition { hangar: id });
        };
        if usize::from(state.launched) != counted {
            return Err(HangarInvariant::LaunchedCount {
                hangar: id,
                cached: state.launched,
                counted,
            });
        }
        if state.total() > usize::from(rules.spec.max_count) {
            return Err(HangarInvariant::OverCapacity {
                hangar: id,
                total: state.total(),
                max_count: rules.spec.max_count,
            });
        }
        let above = state
            .docked
            .iter()
            .any(|record| record.hp > rules.full.hp || record.shields > rules.full.shields);
        if above {
            return Err(HangarInvariant::DockedAboveMaxima { hangar: id });
        }
        Ok(())
    }

    /// Clears the hangar's target, and with it the channel, when the target is no longer a valid
    /// target of the hangar unit's launch weapon (D-6): dead, gone, carried, housed, unseen or
    /// outside its target flags. Orders given to the hangar unit do not clear it.
    fn check_hangar_target(&mut self, id: EntityId) {
        let target = self
            .entities
            .get(&id)
            .and_then(|e| e.hangar.as_ref())
            .and_then(|state| state.target);
        let Some(target) = target else {
            return;
        };
        if self.may_attack(id, target) {
            return;
        }
        if let Some(state) = self.entities.get_mut(&id).and_then(|e| e.hangar.as_mut()) {
            state.target = None;
            state.channel = None;
        }
    }

    /// Counts the channel down and runs its try when it is due (D-5); a hangar unit without a
    /// launch weapon has no channel.
    fn advance_channel(&mut self, id: EntityId, launch: Option<MagazineLaunch>) {
        let Some(state) = self.entities.get_mut(&id).and_then(|e| e.hangar.as_mut()) else {
            return;
        };
        let Some(channel) = state.channel.as_mut() else {
            return;
        };
        let Some(launch) = launch else {
            state.channel = None;
            return;
        };
        channel.next_try_in = channel.next_try_in.saturating_sub(1);
        if channel.next_try_in == 0 {
            self.launch_try(id, launch.interval_ticks);
        }
    }

    /// Queues builds while the hangar's switch is on (`HangarState::auto_build`, plan
    /// `2026-12-01-death-spawns-and-hangar-commands` D-7), its queue and capacity have room, and
    /// the owner can pay the member's cost now, charging each build as it is queued (D-5,
    /// [`try_queue_build`]).
    fn auto_build(&mut self, id: EntityId, owner: PlayerId, rules: &HangarRules) {
        let Some(slot) = owner.slot() else {
            return;
        };
        let Some(state) = self.entities.get_mut(&id).and_then(|e| e.hangar.as_mut()) else {
            return;
        };
        if !state.auto_build {
            return;
        }
        let resources = &mut self.player_resources[slot];
        while try_queue_build(state, &rules.spec, resources, rules.cost).is_ok() {}
    }
}

/// Whether `active`, a member's active order, already is `wanted`: the same kind on the same
/// target (an attack's origin and a follow's kind compared too).
fn holds_order(active: Option<&UnitOrder>, wanted: &UnitOrder) -> bool {
    match (active, wanted) {
        (
            Some(UnitOrder::AttackTarget { target, origin }),
            UnitOrder::AttackTarget {
                target: want,
                origin: want_origin,
            },
        ) => target == want && origin == want_origin,
        (
            Some(UnitOrder::Follow { target, kind }),
            UnitOrder::Follow {
                target: want,
                kind: want_kind,
            },
        ) => target == want && kind == want_kind,
        _ => false,
    }
}
