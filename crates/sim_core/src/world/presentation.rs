//! Effect presentation facts derived from simulation state (plan
//! `2026-11-09-effect-visualization` WP2).
//!
//! # Responsibility
//! Owns the rules that turn a unit's state into the effect contract of `sim_types::presentation`:
//! its concealment, stance, status flags and barrier fraction, and the [`EffectSource`] of a
//! weapon it fires. Also owns [`PresentationWorkspace`], the provenance the tick records for
//! events that outlive their cause: the source of each projectile in flight, the caster of
//! each hazard zone, and which barriers absorbed damage this tick.
//!
//! # How it fits
//! Combat resolution asks [`weapon_source`] when a weapon releases and records projectile
//! sources; abilities record hazard casters; the snapshot writer (tick phase 10) reads the rest.
//! Nothing here names a unit, mode or effect: modes and abilities are indices into the source
//! archetype's own lists, and burrowing is the catalog flag `UnitModeSpec::burrowed`.
//!
//! # Invariants
//! - Presentation only: nothing here is hashed, consumes randomness or is read by a tick
//!   decision, so recording or dropping it cannot move a checksum (plan D-1).
//! - The workspace holds one entry per live projectile and hazard zone it was told about;
//!   entries leave with their projectile or zone.
//! - Its buffers keep their capacity, so a warmed workspace allocates nothing in steady state.

use crate::catalog::UnitArchetype;
use crate::entity::{Entity, ATTACK_DASH_BUFF_ID, OVERCLOCK_BUFF_ID};
use sim_types::{
    AbilityEffectSpec, AbilitySource, Concealment, EffectSource, EntityId, ModeTransitionView,
    ShapeKind, SimScalar, StanceView, StatusFlags, WeaponKey, WeaponSlot, WeaponSpec,
};

/// The caster and full length of a hazard zone, recorded when it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaRecord {
    /// The caster type and ability that opened the zone.
    pub source: AbilitySource,
    /// Ticks (25 Hz) the zone lasts in all.
    pub total_ticks: u16,
}

// PERFORMANCE RATIONALE:
// Why: Projectile and hazard provenance is needed when a projectile lands or a frame is
// written, but must stay out of the hashed `ActiveProjectile` and `ActiveHazardZone`.
// What: Small flat lists keyed by projectile or hazard ID, searched linearly; the oldest
// projectile, found first, is the one most likely to land, and hazard zones are few.
// Trade-off: O(n) lookups over the entries in flight instead of a map, with no allocation once
// the lists have grown to the battle's high-water mark.
/// Unhashed provenance the tick records for the render snapshot.
#[derive(Debug, Default)]
pub(crate) struct PresentationWorkspace {
    /// The source of each projectile in flight, in launch order.
    projectile_sources: Vec<(u32, EffectSource)>,
    /// The caster of each open hazard zone, in opening order.
    area_records: Vec<(u32, AreaRecord)>,
    /// Entities whose barrier absorbed damage since the last snapshot, possibly repeated.
    barrier_hits: Vec<EntityId>,
}

impl PresentationWorkspace {
    /// Records the source of the projectile `id` just launched.
    pub(crate) fn record_projectile(&mut self, id: u32, source: EffectSource) {
        self.projectile_sources.push((id, source));
    }

    /// Removes and returns the source of projectile `id`, which has left the world.
    pub(crate) fn take_projectile_source(&mut self, id: u32) -> Option<EffectSource> {
        let index = self
            .projectile_sources
            .iter()
            .position(|&(entry, _)| entry == id)?;
        Some(self.projectile_sources.remove(index).1)
    }

    /// Records the caster of the hazard zone `id` just opened.
    pub(crate) fn record_area(&mut self, id: u32, record: AreaRecord) {
        self.area_records.push((id, record));
    }

    /// Forgets hazard zone `id`, which has closed.
    pub(crate) fn forget_area(&mut self, id: u32) {
        self.area_records.retain(|&(entry, _)| entry != id);
    }

    /// The caster record of hazard zone `id`, if it was opened by a cast.
    pub(crate) fn area(&self, id: u32) -> Option<AreaRecord> {
        self.area_records
            .iter()
            .find(|&&(entry, _)| entry == id)
            .map(|&(_, record)| record)
    }

    /// Notes that the barrier of `entity` absorbed damage this tick.
    pub(crate) fn record_barrier_hit(&mut self, entity: EntityId) {
        self.barrier_hits.push(entity);
    }

    /// Sorts and deduplicates this tick's barrier hits, for [`Self::barrier_hit`] lookups.
    pub(crate) fn settle_barrier_hits(&mut self) {
        self.barrier_hits.sort_unstable();
        self.barrier_hits.dedup();
    }

    /// Whether the barrier of `entity` absorbed damage this tick. Call after
    /// [`Self::settle_barrier_hits`].
    pub(crate) fn barrier_hit(&self, entity: EntityId) -> bool {
        self.barrier_hits.binary_search(&entity).is_ok()
    }

    /// Ends the frame: this tick's barrier hits have been published.
    pub(crate) fn clear_barrier_hits(&mut self) {
        self.barrier_hits.clear();
    }
}

/// Index of the mode named `name` in `arch.modes`, if it is declared there.
pub(crate) fn mode_index(arch: &UnitArchetype, name: &str) -> Option<u8> {
    arch.modes
        .iter()
        .position(|mode| mode.mode_name == name)
        .and_then(|index| u8::try_from(index).ok())
}

/// The slot under which the ground-plane weapon of `arch` is authored, which a mode's weapon
/// override also takes.
///
/// A structure definition authors only `ground_weapon`. A unit definition's `ground_weapon`
/// defaults to its `weapon`, so a unit whose two are equal authored `weapon` ([`WeaponSlot::Primary`],
/// also for a unit with neither, whose only ground weapon is a mode's override) and one whose
/// two differ authored a separate `ground_weapon`.
fn ground_slot(arch: &UnitArchetype) -> WeaponSlot {
    if matches!(arch.shape, ShapeKind::BuildingDome { .. }) || arch.ground_weapon != arch.weapon {
        WeaponSlot::Ground
    } else {
        WeaponSlot::Primary
    }
}

/// The [`EffectSource`] of `spec`, a weapon `entity` (of archetype `arch`) resolved through
/// `UnitArchetype::weapon_against`, which returns a reference into `arch` or into the entity's
/// active mode weapon.
///
/// The slot is found by identity: the archetype's air weapon is [`WeaponSlot::Air`]; the mode
/// override and the archetype's ground weapon take [`ground_slot`]. Only an override names the
/// mode it belongs to (the entity's active mode, if `arch.modes` declares it).
pub(crate) fn weapon_source(
    arch: &UnitArchetype,
    entity: &Entity,
    spec: &WeaponSpec,
) -> EffectSource {
    let is_air = arch
        .air_weapon
        .as_ref()
        .is_some_and(|air| std::ptr::eq(air, spec));
    let is_override = entity
        .active_siege_weapon()
        .is_some_and(|mode_weapon| std::ptr::eq(mode_weapon, spec));
    let weapon = if is_air {
        WeaponKey {
            slot: WeaponSlot::Air,
            mode: None,
        }
    } else {
        WeaponKey {
            slot: ground_slot(arch),
            mode: if is_override {
                entity
                    .active_mode
                    .as_deref()
                    .and_then(|name| mode_index(arch, name))
            } else {
                None
            },
        }
    };
    EffectSource {
        unit_type: arch.unit_type_id,
        weapon,
    }
}

/// How `entity` (of archetype `arch`) is hidden (plan D-3).
///
/// [`Concealment::Burrowed`] when it is concealed and either its active mode is marked
/// `burrowed`, or it has no active mode and its archetype's concealment comes from a
/// burrowing cloak toggle (`CloakToggle { is_burrow: true }`); [`Concealment::Cloaked`] for any
/// other concealment.
pub(crate) fn concealment(entity: &Entity, arch: &UnitArchetype) -> Concealment {
    // A status's reveal shows the unit as seen (plan `2026-11-10-spellcaster-mechanics` D-11).
    if !entity.is_concealed() {
        return Concealment::None;
    }
    let burrowed = match entity.active_mode.as_deref() {
        Some(name) => arch.get_mode(name).is_some_and(|mode| mode.burrowed),
        None => arch.abilities.iter().any(|ability| {
            matches!(
                ability.effect,
                AbilityEffectSpec::CloakToggle {
                    is_burrow: true,
                    ..
                }
            )
        }),
    };
    if burrowed {
        Concealment::Burrowed
    } else {
        Concealment::Cloaked
    }
}

/// The stance of `entity` (of archetype `arch`): its active mode and the mode switch it
/// channels, if any (plan D-1).
///
/// A switch is published while a `SwitchMode` channel of a declared mode runs: it enters
/// [`Entity::settled_mode`] (`None` when it leaves the active mode), and its nominal length is
/// the mode's `leave_cast_ticks` when leaving and `cast_ticks` otherwise.
pub(crate) fn stance(entity: &Entity, arch: &UnitArchetype) -> StanceView {
    let active = entity.active_mode.as_deref();
    let transition = match entity.pending_ability {
        Some((index, _, _)) if entity.channeling_remaining_ticks > 0 => arch
            .abilities
            .get(usize::from(index))
            .and_then(|ability| match &ability.effect {
                AbilityEffectSpec::SwitchMode { mode_name } => arch
                    .get_mode(mode_name)
                    .map(|mode| (mode, active == Some(mode_name.as_str()))),
                _ => None,
            })
            .map(|(mode, leaving)| ModeTransitionView {
                entering: entity
                    .settled_mode(arch)
                    .and_then(|settled| mode_index(arch, &settled.mode_name)),
                remaining_ticks: entity.channeling_remaining_ticks,
                nominal_ticks: if leaving {
                    mode.leave_cast_ticks
                } else {
                    mode.cast_ticks
                },
            }),
        _ => None,
    };
    StanceView {
        mode: active.and_then(|name| mode_index(arch, name)),
        transition,
    }
}

/// The status flags of `entity`; `barrier_hit` says whether its barrier absorbed damage this
/// tick.
pub(crate) fn status(entity: &Entity, barrier_hit: bool) -> StatusFlags {
    StatusFlags {
        overclock: entity
            .active_buffs
            .iter()
            .any(|buff| buff.buff_id == OVERCLOCK_BUFF_ID),
        dash: entity
            .active_buffs
            .iter()
            .any(|buff| buff.buff_id == ATTACK_DASH_BUFF_ID),
        slowed: entity
            .active_buffs
            .iter()
            .any(|buff| buff.speed_multiplier < SimScalar::ONE),
        barrier_up: entity.barrier.is_raised(),
        barrier_hit,
        damage_over_time: false,
        stunned: false,
        range_reduced: false,
        revealed: false,
    }
}

/// Fraction of the raised barrier's absorption that `entity` (of archetype `arch`) has left;
/// 0.0 while the barrier is down or the archetype has none.
pub(crate) fn barrier_ratio(entity: &Entity, arch: &UnitArchetype) -> f32 {
    match arch.capabilities.barrier.as_ref() {
        Some(spec) if entity.barrier.is_raised() && spec.absorb_limit > SimScalar::ZERO => {
            (entity.barrier.absorb_left / spec.absorb_limit)
                .to_f32()
                .clamp(0.0, 1.0)
        }
        _ => 0.0,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use sim_types::{EffectSource, UnitTypeId};

    fn source(id: u16) -> EffectSource {
        EffectSource {
            unit_type: UnitTypeId(id),
            weapon: WeaponKey::default(),
        }
    }

    #[test]
    fn a_projectile_source_is_taken_once() {
        let mut workspace = PresentationWorkspace::default();
        workspace.record_projectile(4, source(1));
        workspace.record_projectile(5, source(2));
        assert_eq!(workspace.take_projectile_source(5), Some(source(2)));
        assert_eq!(workspace.take_projectile_source(5), None);
        assert_eq!(workspace.take_projectile_source(4), Some(source(1)));
    }

    #[test]
    fn an_area_record_lasts_until_forgotten() {
        let mut workspace = PresentationWorkspace::default();
        let record = AreaRecord {
            source: AbilitySource {
                unit_type: UnitTypeId(3),
                ability: 1,
            },
            total_ticks: 71,
        };
        workspace.record_area(9, record);
        assert_eq!(workspace.area(9), Some(record));
        workspace.forget_area(9);
        assert_eq!(workspace.area(9), None);
    }

    #[test]
    fn barrier_hits_are_looked_up_after_settling_and_cleared_per_frame() {
        let mut workspace = PresentationWorkspace::default();
        for id in [7, 2, 7] {
            workspace.record_barrier_hit(EntityId(id));
        }
        workspace.settle_barrier_hits();
        assert!(workspace.barrier_hit(EntityId(7)));
        assert!(workspace.barrier_hit(EntityId(2)));
        assert!(!workspace.barrier_hit(EntityId(3)));
        workspace.clear_barrier_hits();
        assert!(!workspace.barrier_hit(EntityId(7)));
    }
}
