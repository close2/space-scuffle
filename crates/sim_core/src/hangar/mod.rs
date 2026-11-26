//! Hangars: units that carry a magazine of other units, launch them with a launch weapon and
//! recall them (plan `2026-11-28-carrier-interceptors`; `StarCraft` II's `CAbilArmMagazine`, the
//! Carrier's interceptors).
//!
//! # Responsibility
//! Owns the authoritative hangar state of a hangar unit ([`HangarState`]: its docked members as
//! records, its launched count, its build queue, its target and launch channel), the link of a
//! launched member to its hangar ([`HangarLink`]), and the tick phase that directs, docks, builds
//! and launches members ([`SimWorld::step_hangars`](crate::SimWorld)), with the death hook that
//! kills launched members with their hangar.
//!
//! # How it fits
//! A hangar is a capability of an archetype (`ArchetypeCapabilities::hangar`); its unit's weapon
//! is a launch weapon (`WeaponSpec::magazine_launch`), whose release `dispatch_shot` turns into a
//! launch channel here instead of a strike. A launched member is an ordinary unit entity: it
//! flies the existing air movement under the existing `AttackTarget` and `Follow` orders, which
//! this module writes (D-6, D-10); no movement or acquisition code knows about hangars. The
//! phase runs after vision and before acquisition, so a member launched or redirected this tick
//! is acquired, moved and fires through the ordinary phases of the same tick.
//!
//! # Invariants
//! - A docked member is a record, never an entity; each launch creates a new entity id.
//! - `HangarState::launched` equals the number of living entities whose `hangar_link` names the
//!   hangar unit; the members pass checks it every tick ([`HangarInvariant::LaunchedCount`]).
//! - Docked, launched and queued members together never exceed the hangar's `max_count`.
//! - Every member's owner is its hangar unit's owner.
//! - Hangars and members are visited in ascending entity id; nothing in the phase allocates in
//!   steady state except the entity a launch creates.

mod launch;
mod phase;

use sim_types::{EntityId, PlayerId, SimScalar};

pub(crate) use phase::HangarScratch;

/// A hangar unit's magazine (plan `2026-11-28-carrier-interceptors` D-3). Present exactly on
/// entities whose archetype has a hangar; hashed in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HangarState {
    /// Docked members in docking order (oldest first); launches take the oldest. Capacity is
    /// reserved for `max_count` at creation, so docking never allocates.
    pub docked: Vec<DockedMember>,
    /// Launched members alive in the world; equals the number of entities whose `hangar_link`
    /// names this unit (checked every hangar phase, D-8).
    pub launched: u8,
    /// Builds queued and paid for, the one in progress included.
    pub queued: u8,
    /// Ticks the build in progress has run; 0 when `queued` is 0.
    pub build_progress: u16,
    /// The target members are sent at: the target of the latest launch release, cleared when it
    /// stops being a valid target of the hangar unit's launch weapon (D-6).
    pub target: Option<EntityId>,
    /// The launch channel of the latest release, while it has tries left (D-4, D-5).
    pub channel: Option<LaunchChannel>,
}

impl HangarState {
    /// An empty hangar with room for `max_count` docked records; the spawn stocks its free
    /// start members (D-3).
    #[must_use]
    pub fn with_capacity(max_count: u8) -> Self {
        Self {
            docked: Vec::with_capacity(usize::from(max_count)),
            launched: 0,
            queued: 0,
            build_progress: 0,
            target: None,
            channel: None,
        }
    }

    /// Members docked, launched and queued together, the count `max_count` bounds.
    #[must_use]
    pub fn total(&self) -> usize {
        self.docked.len() + usize::from(self.launched) + usize::from(self.queued)
    }
}

/// A docked member's vitals: what it takes back into the world at its next launch (D-3). Life
/// and shields in hit points, the regeneration clock in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockedMember {
    /// Life, hit points.
    pub hp: SimScalar,
    /// Shields, hit points.
    pub shields: SimScalar,
    /// Ticks since the member last took damage, the clock its shield regeneration delay reads.
    pub ticks_since_last_damage: u16,
}

/// The tries left of a launch release's channel and the ticks until the next one (D-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchChannel {
    /// Launch tries still to come.
    pub tries_left: u8,
    /// Ticks until the next try.
    pub next_try_in: u16,
}

/// On a launched member: its hangar unit and whether it is on its way home (D-3, D-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HangarLink {
    /// The hangar unit the member belongs to.
    pub hangar: EntityId,
    /// Whether the member is returning to dock (beyond the leash, or with no valid target).
    pub returning: bool,
}

/// A malformed hangar relation found by the hangar phase (plan `2026-11-28-carrier-interceptors`
/// D-8). Reported as a [`crate::weapon_timing::SimulationDiagnostic::HangarInvariant`]; the tick
/// goes on with the hangar untouched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HangarInvariant {
    /// A member links to an entity that is missing or has no hangar.
    #[error("member {member:?} links to {hangar:?}, which is no hangar unit")]
    OrphanLink {
        /// The launched member.
        member: EntityId,
        /// The entity its link names.
        hangar: EntityId,
    },
    /// A member's owner differs from its hangar unit's.
    #[error("member {member:?} of {member_owner:?} links to hangar {hangar:?} of another player")]
    CrossOwner {
        /// The launched member.
        member: EntityId,
        /// The member's owner.
        member_owner: PlayerId,
        /// The hangar unit its link names.
        hangar: EntityId,
    },
    /// The hangar's launched count differs from the members linked to it.
    #[error("hangar {hangar:?} counts {cached} launched members, but {counted} link to it")]
    LaunchedCount {
        /// The hangar unit.
        hangar: EntityId,
        /// Its `launched` field.
        cached: u8,
        /// The living members whose link names it.
        counted: usize,
    },
    /// Docked, launched and queued members exceed the hangar's `max_count`.
    #[error("hangar {hangar:?} holds {total} members, more than its {max_count}")]
    OverCapacity {
        /// The hangar unit.
        hangar: EntityId,
        /// Docked, launched and queued together.
        total: usize,
        /// The hangar's `max_count`.
        max_count: u8,
    },
    /// A docked record holds more life or shields than the member archetype allows.
    #[error("hangar {hangar:?} docks a member above the member's maxima")]
    DockedAboveMaxima {
        /// The hangar unit.
        hangar: EntityId,
    },
    /// A hangar unit's archetype, its hangar or its member is missing from the catalog.
    #[error("hangar {hangar:?}: its archetype, hangar or member is missing from the catalog")]
    MissingDefinition {
        /// The hangar unit.
        hangar: EntityId,
    },
    /// A launch could not create the member's entity; the record stays docked.
    #[error("hangar {hangar:?} could not launch a member: {source}")]
    Launch {
        /// The hangar unit.
        hangar: EntityId,
        /// The spawn's refusal, boxed so the error stays small.
        source: Box<crate::production::ProductionInvariant>,
    },
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{DockedMember, HangarInvariant, HangarLink, LaunchChannel};
    use crate::weapon_timing::SimulationDiagnostic;
    use crate::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        EntityId, HangarDeath, HangarSpec, MagazineLaunch, MoverType, PlayerId, SimScalar, SimVec2,
        UnitTypeId,
    };

    /// A world whose first two plain air units are a hangar unit and its member (plan
    /// `2026-11-28-carrier-interceptors` D-2, the 5.0.14 Carrier's values), found by property.
    fn hangar_world() -> (SimWorld, UnitTypeId, UnitTypeId) {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        let mut world: SimWorld = crate::test_support::world(map, 0x0B_A7);
        let mut air: Vec<UnitTypeId> = world
            .catalog
            .archetypes()
            .filter(|arch| {
                arch.mover_type == MoverType::Air
                    && arch.weapon.is_some()
                    && arch.max_speed > SimScalar::ZERO
            })
            .map(|arch| arch.unit_type_id)
            .collect();
        air.sort();
        let (host, member) = (air[0], air[1]);
        let arch = world.catalog.get_mut(host).expect("the host");
        arch.capabilities.hangar = Some(HangarSpec {
            member,
            max_count: 8,
            start_count: 4,
            build_ticks: 214,
            queue_size: 5,
            auto_build: true,
            leash: SimScalar::from_i32(12),
            on_death: HangarDeath::KillMembers,
        });
        if let Some(weapon) = arch.weapon.as_mut() {
            weapon.magazine_launch = Some(MagazineLaunch {
                tries: 8,
                interval_ticks: 7,
            });
        }
        let member_arch = world.catalog.get_mut(member).expect("the member");
        // The member's weapon reaches both planes, so every target here is valid for it.
        let mut beam = member_arch.weapon.clone().expect("an armed member");
        beam.target_flags = sim_types::AttributeFlags::all();
        member_arch.weapon = Some(beam.clone());
        member_arch.ground_weapon = Some(beam.clone());
        member_arch.air_weapon = Some(beam);
        member_arch.supply_cost = 0;
        member_arch.mineral_cost = 15;
        (world, host, member)
    }

    fn spawn(world: &mut SimWorld, kind: UnitTypeId, owner: PlayerId, x: i32) -> EntityId {
        world
            .spawn_unit(kind, owner, SimVec2::from_i32(x, 16))
            .expect("open ground")
    }

    /// D-4: a release during a running channel on the same target starts nothing; a release at
    /// another target restarts the tries, with its first try at once.
    #[test]
    fn a_release_restarts_the_channel_only_on_a_new_target() {
        let (mut world, host, member) = hangar_world();
        let carrier = spawn(&mut world, host, PlayerId(0), 8);
        let first = spawn(&mut world, member, PlayerId(1), 12);
        let second = spawn(&mut world, member, PlayerId(1), 14);
        // The members' owner must see the target for it to be a valid one (D-6).
        world.update_vision();
        let launch = MagazineLaunch {
            tries: 8,
            interval_ticks: 7,
        };
        world.start_launch_channel(carrier, first, launch);
        let state = world.entities[&carrier].hangar().expect("hangar");
        assert_eq!(state.docked.len(), 3, "the first try launches at once");
        assert_eq!(
            state.channel,
            Some(LaunchChannel {
                tries_left: 7,
                next_try_in: 7
            })
        );
        world.start_launch_channel(carrier, first, launch);
        let state = world.entities[&carrier].hangar().expect("hangar");
        assert_eq!(
            (state.docked.len(), state.channel.map(|c| c.tries_left)),
            (3, Some(7))
        );
        world.start_launch_channel(carrier, second, launch);
        let state = world.entities[&carrier].hangar().expect("hangar");
        assert_eq!(state.target, Some(second));
        assert_eq!(
            (state.docked.len(), state.channel.map(|c| c.tries_left)),
            (2, Some(7))
        );
    }

    /// H-11: a world differing in one field of the hangar state, a docked record, the channel
    /// or a member's link has another checksum.
    #[test]
    fn every_hangar_field_is_hashed() {
        type Edit = fn(&mut super::HangarState);
        let (mut world, host, member) = hangar_world();
        let carrier = spawn(&mut world, host, PlayerId(0), 8);
        let target = spawn(&mut world, member, PlayerId(1), 12);
        world.update_vision();
        world.start_launch_channel(
            carrier,
            target,
            MagazineLaunch {
                tries: 8,
                interval_ticks: 7,
            },
        );
        let launched = world
            .entities
            .values()
            .find(|e| e.hangar_link.is_some())
            .map(|e| e.id)
            .expect("a launched member");
        let base = world.compute_checksum();
        let edits: [Edit; 9] = [
            |s| s.docked[0].hp -= SimScalar::ONE,
            |s| s.docked[0].shields -= SimScalar::ONE,
            |s| s.docked[0].ticks_since_last_damage = 3,
            |s| s.launched += 1,
            |s| s.queued += 1,
            |s| s.build_progress += 1,
            |s| s.target = None,
            |s| {
                s.channel = Some(LaunchChannel {
                    tries_left: 1,
                    next_try_in: 1,
                });
            },
            |s| {
                s.docked.pop();
            },
        ];
        let saved = world.entities[&carrier].hangar.clone();
        for (index, edit) in edits.iter().enumerate() {
            let hangar = world.entities.get_mut(&carrier).expect("carrier");
            hangar.hangar.clone_from(&saved);
            edit(hangar.hangar.as_mut().expect("hangar"));
            assert_ne!(world.compute_checksum(), base, "hangar edit {index}");
        }
        world.entities.get_mut(&carrier).expect("carrier").hangar = saved;
        assert_eq!(world.compute_checksum(), base, "restored");
        let saved = world.entities[&launched].hangar_link;
        for link in [
            HangarLink {
                hangar: carrier,
                returning: true,
            },
            HangarLink {
                hangar: target,
                returning: false,
            },
        ] {
            world
                .entities
                .get_mut(&launched)
                .expect("member")
                .hangar_link = Some(link);
            assert_ne!(world.compute_checksum(), base, "{link:?}");
        }
        world
            .entities
            .get_mut(&launched)
            .expect("member")
            .hangar_link = saved;
        let record = DockedMember {
            hp: SimScalar::ONE,
            shields: SimScalar::ZERO,
            ticks_since_last_damage: 0,
        };
        world
            .entities
            .get_mut(&carrier)
            .and_then(|e| e.hangar.as_mut())
            .expect("hangar")
            .docked[0] = record;
        assert_ne!(world.compute_checksum(), base);
    }

    /// H-11, D-8: a corrupted launched count is reported as a hangar invariant and leaves the
    /// hangar untouched (its auto-build does not run).
    #[test]
    fn a_corrupted_count_is_reported_and_the_hangar_left_alone() {
        let (mut world, host, _) = hangar_world();
        let carrier = spawn(&mut world, host, PlayerId(0), 8);
        world.player_resources[PlayerId(0).slot().expect("slot")].minerals = 1000;
        world
            .entities
            .get_mut(&carrier)
            .and_then(|e| e.hangar.as_mut())
            .expect("hangar")
            .launched = 2;
        world.step_hangars();
        assert!(world.diagnostics().iter().any(|d| matches!(
            d,
            SimulationDiagnostic::HangarInvariant {
                source: HangarInvariant::LaunchedCount {
                    cached: 2,
                    counted: 0,
                    ..
                },
                ..
            }
        )));
        let state = world.entities[&carrier].hangar().expect("hangar");
        assert_eq!((state.queued, state.docked.len()), (0, 4));
    }
}
