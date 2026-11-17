//! Effect presentation facts: what the client needs to choose and size an effect.
//!
//! # Responsibility
//! Defines the small, fixed-size records the render snapshot carries for the effect system
//! (plan `2026-11-09-effect-visualization` §5.4, D-1, D-3, D-8): how a bubble is concealed
//! ([`Concealment`]), which stance it is in or channelling into ([`StanceView`]), which status
//! effects are on it ([`StatusFlags`]), which unit and weapon caused a weapon event
//! ([`EffectSource`], [`WeaponKey`]), which entities a strike joined ([`StrikeEnds`]), which
//! catalog statuses an entity bears ([`StatusView`]), the geometry of a line splash
//! ([`LineView`]), and the persistent areas and tethers of a frame ([`AreaView`], [`LinkView`]).
//!
//! # How it fits
//! `sim_core`'s snapshot writer fills these after the tick, from state the tick already keeps;
//! the client reads them and looks the effect up in the source unit's presentation block. They
//! name no unit, weapon, mode or effect: modes and abilities are indices into the source
//! archetype's own lists, so a new unit needs data only.
//!
//! # Invariants
//! - Derived output: nothing here is checksummed or read back by the simulation.
//! - Every record is `Copy` and fixed-size, so a warmed snapshot refills without allocating.
//! - Fields added after the first release read their serde default, so older RON records load.

use crate::effects::StatusId;
use crate::ids::{EntityId, PlayerId, UnitTypeId};
use serde::{Deserialize, Serialize};

/// How a bubble is hidden from players without detection (plan D-3).
///
/// Data, not identity: `Burrowed` comes from the catalog flag `UnitModeSpec::burrowed` (or
/// a burrowing cloak toggle), never from a unit name.
///
/// Older RON records carried a boolean `cloaked` field instead; `BubbleRenderInstance` reads
/// that key as this type, `false` as [`Concealment::None`] and `true` as
/// [`Concealment::Cloaked`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Concealment {
    /// Seen by anyone with vision of its cell.
    #[default]
    #[serde(alias = "false")]
    None,
    /// Cloaked: seen by its enemies only where they detect.
    #[serde(alias = "true")]
    Cloaked,
    /// Burrowed underground: seen by its enemies only where they detect.
    Burrowed,
}

impl Concealment {
    /// Whether players without detection over the bubble cannot see it.
    #[must_use]
    pub const fn is_concealed(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A mode switch under way: the channel that ends in a stance change.
///
/// The client shows progress `clamp((nominal - remaining) / nominal, 0, 1)`; a random extra
/// drawn for the channel reads as a short hold before the morph begins (plan D-1).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeTransitionView {
    /// Index into the archetype's `modes` of the mode the channel enters; `None` when it
    /// returns to the default stance.
    pub entering: Option<u8>,
    /// Ticks (25 Hz) of the channel still to run, counting the random extra.
    pub remaining_ticks: u16,
    /// The direction's nominal ticks from the catalog (`cast_ticks` to enter,
    /// `leave_cast_ticks` to leave), without the random extra.
    pub nominal_ticks: u16,
}

/// The stance of one bubble: its current mode and any switch under way.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StanceView {
    /// Index into the archetype's `modes` of the mode the unit is in now; `None` for the
    /// default stance (and for a stance with no authored mode entry).
    #[serde(default)]
    pub mode: Option<u8>,
    /// The mode switch being channelled, if any.
    #[serde(default)]
    pub transition: Option<ModeTransitionView>,
}

/// The status effects a presentation block can name (`status: { <kind>: "<effect>" }`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StatusKind {
    /// The overclock (stim) buff is running.
    Overclock,
    /// An attack dash is running.
    Dash,
    /// A movement-slowing debuff is running.
    Slowed,
    /// The damage-absorbing barrier is raised.
    Barrier,
    /// A status dealing damage over time is running.
    DamageOverTime,
    /// A status stunning the unit is running.
    Stunned,
    /// A status reducing weapon range is running.
    RangeReduced,
    /// A status revealing the unit through cloak and burrow is running.
    Revealed,
}

/// Whether a flag is off; the spell flags are then left out of a serialized frame, so frames
/// without spells keep the bytes they had before the flags existed.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes the field by reference"
)]
const fn is_false(flag: &bool) -> bool {
    !*flag
}

/// Status effects active on a bubble this frame.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent presentation facts, each driving its own effect"
)]
pub struct StatusFlags {
    /// The overclock (stim) buff is running ([`StatusKind::Overclock`]).
    #[serde(default)]
    pub overclock: bool,
    /// An attack dash is running ([`StatusKind::Dash`]).
    #[serde(default)]
    pub dash: bool,
    /// A buff slowing movement below normal speed is running ([`StatusKind::Slowed`]).
    #[serde(default)]
    pub slowed: bool,
    /// The barrier is raised and absorbing ([`StatusKind::Barrier`]).
    #[serde(default)]
    pub barrier_up: bool,
    /// The barrier absorbed damage during this tick (an absorb flash).
    #[serde(default)]
    pub barrier_hit: bool,
    /// A status dealing damage over time is running ([`StatusKind::DamageOverTime`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub damage_over_time: bool,
    /// A stunning status is running ([`StatusKind::Stunned`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub stunned: bool,
    /// A range-reducing status is running ([`StatusKind::RangeReduced`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub range_reduced: bool,
    /// A revealing status is running ([`StatusKind::Revealed`]).
    #[serde(default, skip_serializing_if = "is_false")]
    pub revealed: bool,
}

impl StatusFlags {
    /// Whether the status `kind` is active.
    #[must_use]
    pub const fn has(self, kind: StatusKind) -> bool {
        match kind {
            StatusKind::Overclock => self.overclock,
            StatusKind::Dash => self.dash,
            StatusKind::Slowed => self.slowed,
            StatusKind::Barrier => self.barrier_up,
            StatusKind::DamageOverTime => self.damage_over_time,
            StatusKind::Stunned => self.stunned,
            StatusKind::RangeReduced => self.range_reduced,
            StatusKind::Revealed => self.revealed,
        }
    }
}

/// Which of an archetype's weapon slots fired, as its presentation block names them.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum WeaponSlot {
    /// The archetype's `weapon` (a unit authored with one weapon for the ground plane).
    #[default]
    Primary,
    /// The archetype's `ground_weapon` (authored separately from `weapon`, as every structure's
    /// ground weapon is).
    Ground,
    /// The archetype's `air_weapon`.
    Air,
}

/// The weapon of an archetype an event came from: a slot, and the mode whose weapon override
/// replaced that slot, if any.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct WeaponKey {
    /// The slot the weapon is authored in, or the slot a mode's override replaces.
    pub slot: WeaponSlot,
    /// Index into the archetype's `modes` of the mode whose override fired; `None` for the
    /// archetype's own weapon.
    #[serde(default)]
    pub mode: Option<u8>,
}

/// The unit type and weapon a weapon event came from (plan D-1).
///
/// Captured when the event is produced, so it outlives its source entity.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct EffectSource {
    /// Archetype of the attacker.
    pub unit_type: UnitTypeId,
    /// The attacker's weapon that fired.
    pub weapon: WeaponKey,
}

/// The unit type and ability that opened an area or a tether (plan D-8).
///
/// The client looks the ability's name up in the source unit's presentation block
/// (`abilities: { "<ability name>": "<effect>" }`).
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct AbilitySource {
    /// Archetype of the caster.
    pub unit_type: UnitTypeId,
    /// Index into the caster archetype's `abilities`.
    pub ability: u8,
}

/// The two entities an instant strike joined: who struck and who was struck (plan
/// `2026-11-17-effect-snapshot-identity` D-2).
///
/// Carried by the instant weapon events (a tracer, a beam, a blow) so a client can draw the
/// strike on the struck entity's own bubble, and follow both ends of a held beam, without
/// searching by position. Both are the entities' ids on the strike's tick; the struck entity may
/// have died on that tick, in which case it is absent from that tick's bubbles.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrikeEnds {
    /// The entity whose weapon struck.
    pub attacker: EntityId,
    /// The entity the strike hit.
    pub target: EntityId,
}

/// One catalog status borne by one published entity (plan `2026-11-17-effect-snapshot-identity`
/// D-5).
///
/// [`StatusFlags`] tells a status by its kind only; this tells it by identity, so a client can
/// choose the look a status's name has (Fungal Growth's apart from Psionic Storm's, both
/// damage-over-time statuses).
///
/// # Invariants
/// - The derived order is `(entity, status)`: [`crate::RenderSnapshot::statuses`] is sorted by
///   it and holds each pair at most once.
/// - `status` is a dense index of the spell catalog the simulation runs (`statuses.ron` in name
///   order); a client resolves names against the same catalog.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusView {
    /// The entity bearing the status; always one of the frame's bubbles.
    pub entity: EntityId,
    /// Which catalog status it bears.
    pub status: StatusId,
}

/// The whole line a line splash searches along, carried by each of its points' events.
///
/// Positions are `[x, y, altitude]` in world units.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineView {
    /// Where the offsets are measured from (the attacker or the struck point).
    pub from: [f32; 3],
    /// The line's far end: the origin moved by the largest offset along the line's axis.
    pub to: [f32; 3],
    /// Distance in world units between consecutive search points; 0 for a single point.
    pub spacing: f32,
    /// Which search point of the line this event is, from 0 in the order searched.
    pub index: u8,
    /// How many search points the line has.
    pub count: u8,
}

/// A persistent area on the ground (a point-anchored effect persistent that searches, such as a
/// storm), refreshed every frame.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaView {
    /// The area's identifier, stable for its lifetime.
    pub id: u32,
    /// Player who opened it.
    pub owner: PlayerId,
    /// Centre `[x, y, altitude]` in world units.
    pub center: [f32; 3],
    /// Radius in world units.
    pub radius: f32,
    /// Ticks (25 Hz) the area still lasts.
    pub remaining_ticks: u16,
    /// Ticks the area lasts in all.
    pub total_ticks: u16,
    /// The caster type and ability that opened it; `None` when the area was placed by other
    /// means (a scenario or a test).
    pub source: Option<AbilitySource>,
}

/// A spell missile in flight presented as a projectile, refreshed every frame (plan
/// `2026-11-10-spellcaster-mechanics` D-11, D-19).
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellMissileView {
    /// The missile's identifier, stable for its flight.
    pub id: u32,
    /// Player whose cast launched it.
    pub owner: PlayerId,
    /// Position `[x, y, altitude]` in world units.
    pub position: [f32; 3],
    /// The caster type and ability that launched it.
    pub source: AbilitySource,
}

/// A tether between two entities, refreshed every frame. Its ends follow the two bubbles.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkView {
    /// The entity the tether starts at.
    pub from: EntityId,
    /// The entity the tether ends at.
    pub to: EntityId,
    /// Player who made it.
    pub owner: PlayerId,
    /// The caster type and ability that made it.
    pub source: AbilitySource,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn only_none_is_unconcealed() {
        assert!(!Concealment::None.is_concealed());
        assert!(Concealment::Cloaked.is_concealed());
        assert!(Concealment::Burrowed.is_concealed());
    }

    #[test]
    fn the_legacy_flag_reads_as_concealment() {
        let read = |text: &str| ron::from_str::<Concealment>(text).expect("a valid concealment");
        assert_eq!(read("false"), Concealment::None);
        assert_eq!(read("true"), Concealment::Cloaked);
        assert_eq!(read("Burrowed"), Concealment::Burrowed);
    }

    #[test]
    fn status_views_order_by_entity_then_status() {
        let view = |entity, status| StatusView {
            entity: EntityId(entity),
            status: StatusId(status),
        };
        let mut views = vec![view(4, 0), view(2, 7), view(4, 1), view(2, 3), view(9, 0)];
        views.sort_unstable();
        assert_eq!(
            views,
            vec![view(2, 3), view(2, 7), view(4, 0), view(4, 1), view(9, 0)],
            "the entity orders first, the status within one entity"
        );
        assert!(view(1, 9) < view(2, 0));
    }

    #[test]
    fn each_status_kind_reads_its_own_flag() {
        let flags = StatusFlags {
            overclock: true,
            dash: false,
            slowed: true,
            barrier_up: false,
            barrier_hit: true,
            damage_over_time: true,
            stunned: false,
            range_reduced: true,
            revealed: false,
        };
        assert!(flags.has(StatusKind::DamageOverTime));
        assert!(!flags.has(StatusKind::Stunned));
        assert!(flags.has(StatusKind::RangeReduced));
        assert!(!flags.has(StatusKind::Revealed));
        assert!(flags.has(StatusKind::Overclock));
        assert!(!flags.has(StatusKind::Dash));
        assert!(flags.has(StatusKind::Slowed));
        assert!(
            !flags.has(StatusKind::Barrier),
            "a hit without a raised barrier is not the barrier status"
        );
    }
}
