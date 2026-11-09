//! Player intent, unit orders, and lockstep tick input bundles.

use crate::ids::{EntityId, PlayerId, ResourceNodeId, Tick, UnitTypeId};
use crate::math::{SimScalar, SimVec2};
use crate::weapon::WeaponSpec;
use serde::{Deserialize, Serialize};

const fn default_true() -> bool {
    true
}

/// Who chose the target of an attack order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttackOrigin {
    /// A player's focus-fire order: only impossibility ends it.
    #[default]
    Explicit,
    /// Chosen by automatic targeting or retaliation: a better target may replace it.
    Acquired,
}

/// Concrete order issued to an individual unit or selection group.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitOrder {
    /// Clear orders and enter idle scanning state.
    Idle,
    /// Move to target position, ignoring hostile encounters.
    Move {
        /// Ground target coordinates.
        target: SimVec2,
    },
    /// Move toward destination, engaging any hostile target entering scan range.
    AttackMove {
        /// Ground target coordinates.
        target: SimVec2,
    },
    /// Focus attack on a specific hostile entity, pursuing if necessary.
    AttackTarget {
        /// Target entity handle.
        target: EntityId,
        /// Who chose this target, which decides whether automatic targeting may replace it.
        #[serde(default)]
        origin: AttackOrigin,
    },
    /// Stay fixed in place; engage targets within range without displacing.
    HoldPosition,
    /// Immediately halt current movement and cancel active backswing.
    Stop,
    /// Activate speed and attack rate buff at the cost of health (e.g., Marine Stimpack).
    UseStim,
    /// Order a production building dome to queue a unit archetype.
    Produce {
        /// Archetype ID to build.
        unit_type: UnitTypeId,
    },
    /// Gather from a resource node and carry the yield to a depot, repeatedly.
    Harvest {
        /// Map resource node to gather from.
        target: ResourceNodeId,
    },
    /// Carry held cargo to the nearest completed friendly depot, then resume gathering.
    ReturnCargo,
    /// Walk to a site, pay and lay a foundation there, then raise it.
    Build {
        /// Archetype ID of structure to construct.
        structure_type: UnitTypeId,
        /// Minimum-corner cell X coordinate.
        cell_x: u32,
        /// Minimum-corner cell Y coordinate.
        cell_y: u32,
    },
    /// Walk to an unfinished friendly foundation and work on it until it completes.
    Construct {
        /// Foundation to work on.
        foundation: EntityId,
    },
    /// Cast an active tactical ability or spell.
    CastAbility {
        /// Index of the ability in the unit's archetype capabilities.
        ability_index: u8,
        /// Target ground coordinates (for ground-targeted spells like Blink or Storm).
        target_pos: Option<SimVec2>,
        /// Target entity handle (for single-target spells like EMP or Snipe).
        target_entity: Option<EntityId>,
    },
    /// Abandon the selected foundations for a partial refund.
    CancelConstruction,
    /// Queue a catalog upgrade at a selected completed research structure.
    Research {
        /// Catalog upgrade identifier (`upgrade_id` in `upgrades.ron`).
        upgrade: u32,
    },
    /// Remove one job from the selected structure's queue for a full refund.
    CancelProduction {
        /// Position in the queue, where zero is the job in progress.
        job_index: u32,
    },
    /// Walk to a friendly carrier and board it.
    Load {
        /// Carrier to board.
        carrier: EntityId,
    },
    /// Set every passenger down around this carrier.
    UnloadAll,
}

impl UnitOrder {
    /// A player's focus-fire order on `target`.
    #[must_use]
    pub const fn attack(target: EntityId) -> Self {
        Self::AttackTarget {
            target,
            origin: AttackOrigin::Explicit,
        }
    }
}

/// Command issued by a player targeting one or more entities during a tick.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerCommand {
    /// Authoring player slot.
    pub player_id: PlayerId,
    /// Entities to which this order applies.
    pub entities: Vec<EntityId>,
    /// Order to execute.
    pub order: UnitOrder,
    /// If true, append to existing unit order queue (Shift-command); otherwise replace.
    pub queue: bool,
}

/// Specifications for temporary overclock / stim combat buffs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OverclockSpec {
    /// Health hit point sacrifice deducted upon activation.
    pub hp_cost: SimScalar,
    /// Duration of the buff in simulation ticks.
    pub duration_ticks: u16,
    /// Movement speed multiplier applied during active overclock.
    pub speed_multiplier: SimScalar,
    /// Attack rate multiplier applied during active overclock (decreases period ticks).
    pub attack_rate_multiplier: SimScalar,
}

/// Specifications for resource harvesting units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarvesterSpec {
    /// Maximum resource units carried per trip.
    pub harvest_capacity: u32,
    /// Ticks required to harvest a single batch from a resource node.
    pub harvest_rate_ticks: u16,
}

/// A damage-absorbing barrier that raises itself when its unit takes damage (`StarCraft` II's
/// Immortal Barrier: the `BarrierDamageResponse` and `ImmortalOverload` behaviours).
///
/// The first damage while the barrier is ready raises it for `duration_ticks` and starts
/// `cooldown_ticks`; while raised it absorbs incoming damage, before armour, until
/// `absorb_limit` hit points have been absorbed. Times are simulation ticks (25 per Faster
/// second); the limit is in hit points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BarrierSpec {
    /// Total damage, in hit points, one raised barrier absorbs before it is spent.
    pub absorb_limit: SimScalar,
    /// Ticks the barrier stays raised, counting the tick that raised it.
    pub duration_ticks: u16,
    /// Ticks from raising the barrier until it can be raised again.
    pub cooldown_ticks: u16,
}

/// A dash an attack order triggers by itself (`StarCraft` II's Charge: the `Charge` autocast
/// augment of the `attack` command and its `Charging` buff).
///
/// While its unit has an attack order on a visible enemy, is not holding position and the dash
/// is off cooldown, the dash starts as soon as the target's gap (centre distance less both
/// collision radii) exceeds `min_gap` and its centre lies closer than `max_distance`: the unit's
/// speed is multiplied by `speed_multiplier` for `duration_ticks` ticks, counting the tick it
/// starts, or until the unit's weapon starts a swing, and `cooldown_ticks` starts. Distances are
/// world units; times are simulation ticks (25 per Faster second).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttackDashSpec {
    /// Factor applied to the unit's movement speed while the dash lasts.
    pub speed_multiplier: SimScalar,
    /// Ticks the dash lasts at most, counting the tick it starts.
    pub duration_ticks: u16,
    /// Ticks from the start of a dash until another may start.
    pub cooldown_ticks: u16,
    /// Edge-to-edge gap to the target, world units, that a dash needs to exceed.
    pub min_gap: SimScalar,
    /// Centre-to-centre distance to the target, world units, below which a dash may start.
    pub max_distance: SimScalar,
}

/// Execution mode and targeting envelope for an active ability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AbilityTargetKind {
    /// Instant cast on the executing unit itself.
    InstantSelf,
    /// Targeted at a ground position in world space.
    TargetPosition,
    /// Targeted at a specific entity handle.
    TargetEntity,
    /// Stance/mode toggle (e.g., Siege Mode or Cloak).
    ToggleMode,
}

/// Mechanical payload executed when an ability triggers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityEffectSpec {
    /// Instant temporary combat buff (Stimpack / Overclock).
    Overclock(OverclockSpec),
    /// Siege Mode channel and stance transformation.
    SiegeModeToggle {
        /// Channel ticks required to transform.
        cast_ticks: u16,
        /// Active weapon configuration while sieged, boxed so this variant stays close in size to
        /// the others.
        sieged_weapon: Box<WeaponSpec>,
    },
    /// Instant tactical teleportation to target coordinate.
    BlinkTeleport {
        /// Maximum Euclidean teleport distance.
        max_range: SimScalar,
        /// Cooldown ticks incurred on cast.
        cooldown_ticks: u16,
    },
    /// Spawns a periodic area-of-effect damage hazard zone at the target point.
    SpawnHazardZone {
        /// Circular radius of effect.
        radius: SimScalar,
        /// Initial detonation delay in ticks before first damage application.
        delay_ticks: u16,
        /// Total lifetime duration of the hazard zone in ticks.
        duration_ticks: u16,
        /// Ticks between consecutive periodic damage pulses.
        interval_ticks: u16,
        /// Damage applied per interval pulse.
        damage_per_interval: SimScalar,
        /// If true, friendly entities also suffer damage.
        #[serde(default = "default_true", alias = "targets_friendly")]
        friendly_fire: bool,
    },
    /// Tactical cloaking or subterranean burrowing.
    CloakToggle {
        /// Energy drain per second while active.
        energy_cost_per_second: SimScalar,
        /// Whether cloaking anchors unit as immobile burrow.
        is_burrow: bool,
    },
    /// Activates or toggles a generalized unit stance/mode.
    SwitchMode {
        /// Canonical mode identifier to enter/toggle (matches `UnitModeSpec.mode_name`).
        mode_name: String,
    },
}

/// Declarative specification of a toggleable unit stance or transformation mode
/// (e.g., Siege Mode, Burrowed Mode, Fighter/Assault Mode, High-Impact Payload, Surveillance Mode, Cloak).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitModeSpec {
    /// Canonical identifier for this mode (e.g., `"siege"`, `"burrowed"`, `"assault"`, `"cloaked"`).
    pub mode_name: String,
    /// Channel duration in ticks required to enter this mode: the time until the mode's stats
    /// (weapon, cloak, stance) apply. In `StarCraft` II that is the end of the morph's `Stats`
    /// section, delay plus duration, which may come before its longest (`Actor`) section ends:
    /// each `InfoArray` section drives its own subsystem (owner answer to Q0069). Siege Mode's
    /// `Stats` and `Actor` sections end together; the Lurker's `Stats` ends at 1.8332 Normal s,
    /// its `Actor` at 2.5.
    #[serde(default)]
    pub cast_ticks: u16,
    /// Channel duration in ticks required to leave this mode back to the default stance.
    #[serde(default)]
    pub leave_cast_ticks: u16,
    /// Greatest random extra added to the entering channel, in ticks (`StarCraft` II
    /// `RandomDelayMax` of the morph; drawn uniformly from `0..=max` with the world's seeded
    /// generator and rounded half up to whole ticks). 0 adds nothing and draws nothing.
    #[serde(default)]
    pub cast_random_delay_max_ticks: SimScalar,
    /// Greatest random extra added to the leaving channel, in ticks, drawn as
    /// `cast_random_delay_max_ticks` is.
    #[serde(default)]
    pub leave_random_delay_max_ticks: SimScalar,
    /// Whether the unit becomes immobile while this mode is active.
    #[serde(default)]
    pub is_immobile: bool,
    /// Whether the unit cannot be pushed by friendly units while this mode is active.
    #[serde(default)]
    pub unpushable: bool,
    /// Whether the unit is cloaked/burrowed (invisible without detection) in this mode.
    #[serde(default)]
    pub is_cloaked: bool,
    /// Optional movement type override while in this mode (e.g., Air -> Ground for transforming fighters).
    #[serde(default)]
    pub mover_type_override: Option<crate::enums::MoverType>,
    /// Optional sight radius override while in this mode (e.g., Surveillance Mode).
    #[serde(default)]
    pub sight_radius_override: Option<SimScalar>,
    /// Optional replacement ground weapon while in this mode (`Some(Some(w))` replaces weapon, `Some(None)` disables ground weapon e.g. when burrowed without spines, `None` keeps default weapon).
    #[serde(default)]
    pub ground_weapon_override: Option<Option<WeaponSpec>>,
    /// Optional replacement air weapon while in this mode.
    #[serde(default)]
    pub air_weapon_override: Option<Option<WeaponSpec>>,
    /// Continuous energy drain per second (25 ticks) while this mode is active; mode deactivates if energy hits 0.
    #[serde(default)]
    pub energy_cost_per_second: SimScalar,
}

/// Static ability specification in the unit archetype.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilitySpec {
    /// Canonical display/debug name of the ability.
    pub name: String,
    /// Targeting mode and envelope.
    pub target_kind: AbilityTargetKind,
    /// Energy deducted from caster pool upon activation.
    pub energy_cost: SimScalar,
    /// Cooldown ticks before ability can be re-cast.
    pub cooldown_ticks: u16,
    /// Maximum casting range in world units (0 if self/instant).
    pub cast_range: SimScalar,
    /// Mechanical effect payload.
    pub effect: AbilityEffectSpec,
}

/// Bundle of optional mechanical capabilities possessed by an archetype.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ArchetypeCapabilities {
    /// Overclock / stim combat buff parameters (if capable).
    pub overclock: Option<OverclockSpec>,
    /// Resource harvesting parameters (if capable).
    pub harvester: Option<HarvesterSpec>,
    /// Damage-absorbing barrier raised on taking damage (if capable).
    #[serde(default)]
    pub barrier: Option<BarrierSpec>,
    /// Speed dash an attack order triggers by itself (if capable).
    #[serde(default)]
    pub attack_dash: Option<AttackDashSpec>,
    /// Whether this structure serves as a resource dropoff node.
    pub is_resource_dropoff: bool,
    /// Whether this structure extracts vespene gas for harvesters.
    #[serde(default)]
    pub is_gas_extractor: bool,
    /// Whether this unit can raise structures of its own faction.
    #[serde(default)]
    pub can_construct: bool,
    /// Passenger slots this unit carries; zero means it is no carrier. A passenger
    /// fills as many slots as its supply cost, and at least one.
    #[serde(default)]
    pub transport_capacity: u32,
    /// Active abilities and spells available to this archetype.
    #[serde(default)]
    pub abilities: Vec<AbilitySpec>,
    /// Declarative transformation stances and operational modes.
    #[serde(default)]
    pub modes: Vec<UnitModeSpec>,
}

/// Network/relay bundle containing all validated player commands scheduled for a tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickInputBundle {
    /// Target simulation tick.
    pub tick: Tick,
    /// Player commands executed at this tick.
    pub commands: Vec<PlayerCommand>,
    /// Expected simulation state checksum of the preceding tick (for desync detection).
    pub state_checksum: Option<u64>,
}

/// Alias for `TickInputBundle` consumed by `sim_core`.
pub type TickBundle = TickInputBundle;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::math::SimScalar;

    #[test]
    fn test_commands_serde() {
        let order = UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_i32(10), SimScalar::from_i32(20)),
        };
        let cmd = PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![EntityId(1), EntityId(2)],
            order,
            queue: false,
        };
        let bundle = TickInputBundle {
            tick: Tick(100),
            commands: vec![cmd],
            state_checksum: Some(0xDEAD_BEEF),
        };

        let ron_str = ron::to_string(&bundle).expect("invariant: valid ron serialization");
        let decoded: TickInputBundle =
            ron::from_str(&ron_str).expect("invariant: valid ron deserialization");
        assert_eq!(bundle, decoded);
    }
}
