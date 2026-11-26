//! Effect presentation facts derived from simulation state (plan
//! `2026-11-09-effect-visualization` WP2).
//!
//! # Responsibility
//! Owns the rules that turn a unit's state into the effect contract of `sim_types::presentation`:
//! its concealment, stance, status flags and barrier fraction, and the [`EffectSource`] of a
//! weapon it fires. Also owns [`PresentationWorkspace`], the provenance the tick records for
//! events that outlive their cause: the source of each projectile in flight, which barriers
//! absorbed damage this tick, and where spell missiles landed this tick (plan
//! `2026-11-17-effect-snapshot-identity` D-6). It also holds each player's attack snapshots:
//! the attackers that struck that player's units from where it did not see them, each in the pose
//! of its last such strike (plan `2026-11-24-attacker-snapshot` D-1, D-4).
//!
//! # How it fits
//! Combat resolution asks [`weapon_source`] when a weapon releases and records projectile
//! sources; the effect engine records each spell missile's landing; the snapshot writer (tick
//! phase 10) reads the rest and drains the landings into events. Combat records attack
//! snapshots as strikes land (phase 7), the vision phase drops those their viewer has seen
//! again (phase 3), and `SimWorld::write_player_view` publishes a viewer's own.
//! Nothing here names a unit, mode or effect: modes and abilities are indices into the source
//! archetype's own lists, and burrowing is the catalog flag `UnitModeSpec::burrowed`.
//!
//! # Invariants
//! - Presentation only: nothing here is hashed, consumes randomness or is read by a tick
//!   decision, so recording or dropping it cannot move a checksum (plan D-1).
//! - The workspace holds one entry per live projectile it was told about; entries leave
//!   with their projectile. Spell-missile landings are held in fixed point and only until the
//!   next snapshot, which `step_into` writes every tick.
//! - Its buffers keep their capacity, so a warmed workspace allocates nothing in steady state.
//! - Attack snapshots are sorted by `(viewer, attacker)`, at most one per pair, and in fixed
//!   point. They last across ticks, but they are presentation like the rest: no tick decision
//!   reads them, which a paired-world test pins (plan `2026-11-24-attacker-snapshot` P-9).

use crate::catalog::UnitArchetype;
use crate::entity::{Entity, ATTACK_DASH_BUFF_ID, OVERCLOCK_BUFF_ID};
use sim_types::{
    AbilityEffectSpec, AbilitySource, Concealment, EffectSource, EntityId, ModeTransitionView,
    PlayerId, ShapeKind, SimAngle, SimScalar, SimVec2, StanceView, StatusFlags, Tick, UnitTypeId,
    WeaponKey, WeaponSlot, WeaponSpec,
};

/// Where a spell missile presented as a projectile landed on this tick: the provenance of a
/// `CombatEventVisual::SpellImpact` (plan `2026-11-17-effect-snapshot-identity` D-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpellImpactRecord {
    /// The landed missile's identifier.
    pub(crate) missile: u32,
    /// Player whose cast launched it.
    pub(crate) owner: PlayerId,
    /// The contact point the impact ran at, in world units.
    pub(crate) contact: SimVec2,
    /// The caster type and ability that launched it.
    pub(crate) source: AbilitySource,
}

/// One player's snapshot of an attacker that struck it unseen (plan
/// `2026-11-24-attacker-snapshot` D-2): by a weapon's damage, a weapon missile's release or a
/// spell's damage or unit-aimed missile, whose attacker is the caster (plan
/// `2026-11-27-launch-and-spell-visibility` D-3). The record does not keep its cause: nothing
/// reads one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttackSnapshotRecord {
    /// The player whose units were struck, and who keeps the image.
    pub(crate) viewer: PlayerId,
    /// The attacker. It may since have moved or died.
    pub(crate) attacker: EntityId,
    /// The tick of the strike, launch or spell damage that made or last refreshed it.
    pub(crate) struck: Tick,
    /// What the image shows.
    pub(crate) pose: SnapshotPose,
}

/// What the snapshot shows of the attacker, as it stood at the strike (plan
/// `2026-11-24-attacker-snapshot` D-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotPose {
    /// The attacker's catalog type, so a dead attacker's image is still built.
    pub(crate) unit_type: UnitTypeId,
    /// The attacker's player.
    pub(crate) owner: PlayerId,
    /// Where it stood, world units.
    pub(crate) position: SimVec2,
    /// Its hull's facing, binary angle.
    pub(crate) heading: SimAngle,
    /// Its turret's facing, binary angle.
    pub(crate) turret: SimAngle,
    /// Its settled mode, an index into the archetype's modes; `None` in its default stance.
    pub(crate) mode: Option<u8>,
    /// Its health as a share of its maximum, in `0..=1`.
    pub(crate) hp_fraction: SimScalar,
    /// Its shields as a share of the archetype's maximum, in `0..=1`; 0 without shields.
    pub(crate) shield_fraction: SimScalar,
}

// PERFORMANCE RATIONALE:
// Why: Projectile provenance is needed when a projectile lands or a frame is written, but
// must stay out of the hashed `ActiveProjectile`.
// What: A small flat list keyed by projectile ID, searched linearly; the oldest projectile,
// found first, is the one most likely to land.
// Trade-off: O(n) lookups over the entries in flight instead of a map, with no allocation once
// the lists have grown to the battle's high-water mark.
/// Unhashed provenance the tick records for the render snapshot.
#[derive(Debug, Default)]
pub(crate) struct PresentationWorkspace {
    /// The source of each projectile in flight, in launch order.
    projectile_sources: Vec<(u32, EffectSource)>,
    /// Entities whose barrier absorbed damage since the last snapshot, possibly repeated.
    barrier_hits: Vec<EntityId>,
    /// Spell missiles that landed since the last snapshot, in the order their impacts ran.
    spell_impacts: Vec<SpellImpactRecord>,
    // PERFORMANCE RATIONALE:
    // Why: Every `Snapshot` strike on an unseen attacker records or refreshes an entry, every
    // tick prunes the list, and every view reads one player's entries in attacker order; none of
    // it may allocate once warm, and the order must not depend on a hasher.
    // What: One `Vec` sorted by `(viewer, attacker)`: a binary search inserts or refreshes in
    // O(log n), one `retain` pass prunes, and one player's entries are a contiguous run.
    // Trade-off: O(n) element moves on an insertion instead of a map's O(log n), for n at most
    // players times attackers, with no allocation once the list holds its high-water mark.
    /// Attack snapshots, sorted by `(viewer, attacker)`, at most one per pair.
    attack_snapshots: Vec<AttackSnapshotRecord>,
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

    /// Notes that a spell missile presented as a projectile landed (its impact ran) this tick.
    pub(crate) fn record_spell_impact(&mut self, record: SpellImpactRecord) {
        self.spell_impacts.push(record);
    }

    /// Removes and yields this tick's landings in the order they ran, keeping the buffer's
    /// capacity.
    pub(crate) fn drain_spell_impacts(&mut self) -> std::vec::Drain<'_, SpellImpactRecord> {
        self.spell_impacts.drain(..)
    }

    /// Records `record`, replacing the entry of the same viewer and attacker if there is one: a
    /// later strike moves the image to the attacker's new pose (plan
    /// `2026-11-24-attacker-snapshot` D-2, refresh).
    pub(crate) fn record_attack_snapshot(&mut self, record: AttackSnapshotRecord) {
        let key = (record.viewer, record.attacker);
        match self
            .attack_snapshots
            .binary_search_by_key(&key, |entry| (entry.viewer, entry.attacker))
        {
            Ok(index) => self.attack_snapshots[index] = record,
            Err(index) => self.attack_snapshots.insert(index, record),
        }
    }

    /// Keeps only the attack snapshots `keep` accepts, in order, keeping the buffer's capacity.
    pub(crate) fn prune_attack_snapshots(
        &mut self,
        keep: impl FnMut(&AttackSnapshotRecord) -> bool,
    ) {
        self.attack_snapshots.retain(keep);
    }

    /// The attack snapshots `viewer` keeps, sorted by attacker.
    pub(crate) fn attack_snapshots_of(
        &self,
        viewer: PlayerId,
    ) -> impl Iterator<Item = &AttackSnapshotRecord> + '_ {
        let start = self
            .attack_snapshots
            .partition_point(|entry| entry.viewer < viewer);
        self.attack_snapshots[start..]
            .iter()
            .take_while(move |entry| entry.viewer == viewer)
    }

    /// Whether any player keeps an attack snapshot.
    pub(crate) fn has_attack_snapshots(&self) -> bool {
        !self.attack_snapshots.is_empty()
    }

    /// Forgets every attack snapshot: the test hook of plan `2026-11-24-attacker-snapshot` P-9,
    /// which shows that no tick decision reads them.
    #[cfg(test)]
    pub(crate) fn forget_attack_snapshots(&mut self) {
        self.attack_snapshots.clear();
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
    weapon_source_with_mode(arch, entity, entity.active_siege_weapon(), spec)
}

/// [`weapon_source`] for a `spec` resolved through `UnitArchetype::weapon_against` with
/// `mode_weapon` standing for the entity's active mode weapon: the weapon itself, or the copy
/// the combat phase steps while it mutates the entity. The override is found by identity with
/// `mode_weapon`.
pub(crate) fn weapon_source_with_mode(
    arch: &UnitArchetype,
    entity: &Entity,
    mode_weapon: Option<&WeaponSpec>,
    spec: &WeaponSpec,
) -> EffectSource {
    let is_air = arch
        .air_weapon
        .as_ref()
        .is_some_and(|air| std::ptr::eq(air, spec));
    let is_override = mode_weapon.is_some_and(|mode_weapon| std::ptr::eq(mode_weapon, spec));
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

    /// An attack snapshot of `attacker` for `viewer`, struck on `tick` at `x`.
    fn snap(viewer: u8, attacker: u32, tick: u32, x: i32) -> AttackSnapshotRecord {
        AttackSnapshotRecord {
            viewer: PlayerId(viewer),
            attacker: EntityId(attacker),
            struck: Tick(tick),
            pose: SnapshotPose {
                unit_type: UnitTypeId(3),
                owner: PlayerId(7),
                position: SimVec2::from_i32(x, 4),
                heading: SimAngle::ZERO,
                turret: SimAngle::ZERO,
                mode: None,
                hp_fraction: SimScalar::ONE,
                shield_fraction: SimScalar::ZERO,
            },
        }
    }

    fn keys(workspace: &PresentationWorkspace, viewer: u8) -> Vec<(u32, u32)> {
        workspace
            .attack_snapshots_of(PlayerId(viewer))
            .map(|entry| (entry.attacker.0, entry.struck.0))
            .collect()
    }

    /// D-4: records are kept sorted by viewer and attacker whatever order they arrive in, and
    /// one viewer's run lists only its own.
    #[test]
    fn attack_snapshots_are_sorted_by_viewer_then_attacker() {
        let mut workspace = PresentationWorkspace::default();
        for (viewer, attacker) in [(1, 9), (0, 5), (1, 2), (3, 1), (0, 1)] {
            workspace.record_attack_snapshot(snap(viewer, attacker, 10, 1));
        }
        assert_eq!(keys(&workspace, 0), [(1, 10), (5, 10)]);
        assert_eq!(keys(&workspace, 1), [(2, 10), (9, 10)]);
        assert_eq!(keys(&workspace, 2), []);
        assert_eq!(keys(&workspace, 3), [(1, 10)]);
        let all: Vec<_> = workspace
            .attack_snapshots
            .iter()
            .map(|entry| (entry.viewer, entry.attacker))
            .collect();
        let mut sorted = all.clone();
        sorted.sort_unstable();
        assert_eq!(all, sorted);
    }

    /// D-2, refresh: a later strike replaces the pose and the tick; one entry per pair.
    #[test]
    fn a_later_strike_refreshes_the_one_entry() {
        let mut workspace = PresentationWorkspace::default();
        workspace.record_attack_snapshot(snap(1, 4, 10, 1));
        workspace.record_attack_snapshot(snap(1, 4, 25, 6));
        let entries: Vec<_> = workspace.attack_snapshots_of(PlayerId(1)).collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].struck, Tick(25));
        assert_eq!(entries[0].pose.position, SimVec2::from_i32(6, 4));
    }

    /// D-2, drop: the prune keeps exactly what its function accepts, in order, and keeps the
    /// buffer's capacity.
    #[test]
    fn the_prune_keeps_what_its_function_accepts() {
        let mut workspace = PresentationWorkspace::default();
        for attacker in 1..=6 {
            workspace.record_attack_snapshot(snap(0, attacker, attacker, 1));
        }
        let capacity = workspace.attack_snapshots.capacity();
        workspace.prune_attack_snapshots(|entry| entry.attacker.0 % 2 == 0);
        assert_eq!(keys(&workspace, 0), [(2, 2), (4, 4), (6, 6)]);
        assert_eq!(workspace.attack_snapshots.capacity(), capacity);
        workspace.prune_attack_snapshots(|_| false);
        assert!(!workspace.has_attack_snapshots());
        assert_eq!(workspace.attack_snapshots.capacity(), capacity);
    }
}
