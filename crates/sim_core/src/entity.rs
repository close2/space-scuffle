//! # Deterministic Entity Representation & Components
//!
//! Models authoritative state, orders, vitals, weapon cycles, and movement
//! for all dynamic mobile units, building domes, and projectiles.
//!
//! ## Invariants Maintained
//! 1. All calculations and component fields strictly use deterministic fixed-point math.
//! 2. No heap allocation occurs during standard per-tick updates.
//! 3. Weapon phases and order transitions strictly obey `StarCraft` II micro mechanics.

use crate::catalog::{UnitArchetype, WeaponSpec};
use crate::movement::MovementMathError;
use crate::weapon_timing::{combined_attack_rate, AttackRate, InvalidBuffAttackRate};
use sim_types::{
    AbilityEffectSpec, AnimState, AttackOrigin, BarrierSpec, EntityId, MoverType, OverclockSpec,
    PlayerId, ResourceKind, ResourceNodeId, ShapeKind, SimAngle, SimScalar, SimVec2, Tick,
    UnitOrder, UnitTypeId,
};
use std::collections::VecDeque;

// PERFORMANCE RATIONALE:
// Why: Standard HashMap iteration is non-deterministic across executions, causing lockstep desyncs.
// Naive sorting of keys on every tick incurs an O(N log N) overhead per query.
// What: Authoritative entities are stored in a sorted `BTreeMap<EntityId, Entity>` with deterministic
// traversal by EntityId.
// Trade-off: BTreeMap has slightly higher pointer-chase overhead compared to contiguous Vecs,
// but guarantees strict, deterministic, allocation-free in-order iteration for checksums and systems.

/// `active_mode` value of the built-in siege toggle, which has no authored mode entry.
const SIEGE_MODE_NAME: &str = "siege";

/// A malformed numeric buff/cost input, distinct from ordinary cast eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EntityAdmissionError {
    /// Checked movement rate/fold preparation failed.
    #[error(transparent)]
    Movement(#[from] MovementMathError),
    /// Authored or mutable fire rate is nonpositive.
    #[error(transparent)]
    AttackRate(#[from] crate::weapon_timing::InvalidAttackRate),
    /// HP/energy costs must not grant resources through a negative payment.
    #[error("ability cost must be nonnegative")]
    NegativeCost,
    /// Prepared resource subtraction cannot be represented.
    #[error("ability resource payment cannot be represented")]
    PaymentOverflow,
    /// An immediate `UseStim` was given to an entity directly. Only `SimWorld` knows the
    /// archetype's Overclock spec in the catalog and whether its research is done, so a stim is
    /// admitted only through a `PlayerCommand` (plan `2026-11-05-sc2-deviation-fixes` B-32).
    #[error(
        "an immediate stim needs the world's catalog and unlocks; submit it as a player command"
    )]
    StimOutsideWorld,
}

#[derive(Clone, Copy)]
pub(crate) struct PreparedOverclock {
    pub spec: OverclockSpec,
    pub hp: SimScalar,
    pub multiplier: SimScalar,
}

/// Represents the active phase of an entity's weapon cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeaponPhase {
    /// Weapon is charged and ready to fire as soon as a target is valid.
    Ready,
    /// Unit or turret is rotating to bring the target within the firing arc.
    RotatingToTarget {
        /// Target entity handle being aligned towards.
        target_id: EntityId,
    },
    /// `DamagePoint` wind-up in progress. Moving before completion cancels the attack without damage.
    ///
    /// A weapon with a damage point of 0 never enters this phase: its swing releases on the tick
    /// it starts (plan `2026-11-04-combat-sc2-alignment` D-3).
    Windup {
        /// Target entity handle.
        target_id: EntityId,
        /// Remaining ticks before shot discharge.
        remaining_ticks: u16,
    },
    /// Damage has been applied; unit is executing the post-shot backswing animation.
    ///
    /// A lock on orders, not a gate on the weapon: the cooldown runs in parallel, and once it
    /// has run out the weapon acts as if `Ready`, so a new swing (or turning towards the
    /// target) ends the backswing early. With nothing to attack it runs to its end. A `Move` or
    /// `Stop` cancels it into `Cooldown` without DPS loss (stutter-step).
    Backswing {
        /// Target entity handle.
        target_id: EntityId,
        /// Remaining ticks in the animation follow-through.
        remaining_ticks: u16,
    },
    /// Weapon is on cooldown following an attack. Unit can move and perform other actions.
    Cooldown,
}

/// The strikes of an attack that land after its release tick (plan
/// `2026-11-05-sc2-deviation-fixes` WP5, B-2).
///
/// In `StarCraft` II a multi-strike weapon is a `CEffectCreatePersistent` whose periods land one
/// strike each: the Zealot's second blade 0.28 Normal s (5 ticks) after the first. Combat
/// resolution lands the first strike on the release tick and keeps the rest here, one every
/// [`WeaponSpec::strike_interval_ticks`]. A later strike lands only if its target is still a valid
/// target on the weapon's plane and, where the weapon names one, within its
/// [`WeaponSpec::followup_max_distance`]; a strike that fails its condition is lost, and the
/// strikes after it still try. The attacker's death or boarding a transport ends the burst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingStrikes {
    /// Entity the attack was released at; every strike of the burst is aimed at it.
    pub target_id: EntityId,
    /// Combat plane of the weapon that released the attack; a strike lands only on a target
    /// still on that plane, and the weapon is resolved against it again for each strike.
    pub plane: MoverType,
    /// Strikes still to land, at least 1 while the burst is pending.
    pub remaining: u8,
    /// Ticks until the next strike lands, at least 1 between ticks.
    pub ticks_until_next: u16,
}

/// An active timed buff or debuff affecting unit stats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveBuff {
    /// Distinct buff identifier (1 = Stimpack, 2 = Concussive Shells).
    pub buff_id: u8,
    /// Remaining duration in ticks.
    pub remaining_ticks: u16,
    /// Nonnegative ordered movement multiplier; direct mutation is validated before motion.
    pub speed_multiplier: SimScalar,
    /// Strictly positive multiplier applied to weapon fire rate / period.
    /// Direct mutation must preserve positivity; invalid rates produce tick diagnostics.
    pub attack_rate_multiplier: SimScalar,
}

/// In-flight physical projectile (e.g. Marauder concussive grenade, Siege Tank shell).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProjectile {
    /// Unique projectile ID.
    pub id: u32,
    /// Player owning the firing unit.
    pub owner: PlayerId,
    /// Attacker entity ID (if known) for retaliation aggro.
    pub attacker_id: Option<EntityId>,
    /// Launch origin position.
    pub source_pos: SimVec2,
    /// Current world position.
    pub current_pos: SimVec2,
    /// Target entity, if homing on a specific unit.
    pub target_id: Option<EntityId>,
    /// Target coordinates in world space.
    pub target_pos: SimVec2,
    /// Locomotion layer of the target, retained so an impact on bare ground keeps its plane.
    pub target_mover: MoverType,
    /// Travel speed per tick.
    pub speed: SimScalar,
    /// Weapon specification for damage resolution.
    pub weapon_spec: WeaponSpec,
    /// Originating unit archetype slug (if known), for unit-specific upgrade bonuses.
    pub source_slug: Option<String>,
    /// Remaining in-flight lifetime in ticks before missile fuels out and expires.
    pub remaining_lifetime_ticks: u16,
    /// Strikes of `weapon_spec` this projectile carries: every strike of the attack when they
    /// land together, one when the weapon spaces its strikes ([`PendingStrikes`]).
    pub strikes: u8,
}

/// What a unit remembers about its movement order between ticks.
///
/// All of this is authoritative: it decides when an order ends, so it is hashed by the checksum.
/// A new order starts it afresh, apart from [`Self::abandoned_target`], which outlives orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MovementMemory {
    /// Destination this unit finished moving to. Others heading there stop against this unit.
    pub arrived_at: Option<SimVec2>,
    /// Shortest remaining route length seen on this order, in world units.
    pub progress_best: Option<SimScalar>,
    /// Waypoint count of the route `progress_tail` was measured on.
    pub progress_legs: u16,
    /// Route length beyond the next waypoint, in world units.
    pub progress_tail: SimScalar,
    /// Ticks since the remaining route length last shortened.
    pub stalled_ticks: u16,
    /// Ticks until this unit may search for a route again.
    pub repath_cooldown: u16,
    /// Enemy this unit gave up chasing, which automatic targeting skips for a while.
    pub abandoned_target: Option<EntityId>,
    /// Ticks until that enemy may be chosen again.
    pub abandoned_ticks: u16,
    /// Side this unit walks along a crowd blocking its target: left of its heading when true.
    pub flank_left: bool,
}

impl MovementMemory {
    /// Clears everything tied to the current order.
    pub(crate) fn restart(&mut self) {
        *self = Self {
            abandoned_target: self.abandoned_target,
            abandoned_ticks: self.abandoned_ticks,
            flank_left: self.flank_left,
            ..Self::default()
        };
    }
}

/// Resources a harvester is holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cargo {
    /// What it holds.
    pub kind: ResourceKind,
    /// How much, in resource units.
    pub amount: u32,
}

/// What a unit remembers about the task order it is working through; see [`crate::economy`].
///
/// All of it decides what the unit does next, so all of it is hashed by the checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UnitTask {
    /// Where the current leg is walking to, in world units. `None` between legs.
    pub goal: Option<SimVec2>,
    /// Legs set off on since the task last got where it was going.
    pub legs: u8,
    /// Ticks left gathering; zero when not gathering.
    pub gather_ticks: u16,
    /// Ticks spent waiting for a site or for ground to clear.
    pub wait_ticks: u16,
    /// Depot a carrying unit is heading for.
    pub depot: Option<EntityId>,
    /// Node last gathered from, where `ReturnCargo` resumes. Outlives orders.
    pub last_node: Option<ResourceNodeId>,
}

/// The state of a unit's damage-absorbing barrier ([`BarrierSpec`], B-41).
///
/// All zero for a unit without the capability and for one whose barrier is ready. It decides
/// how much later damage is absorbed, so the checksum hashes all of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BarrierState {
    /// Ticks the raised barrier stays up, counting this one; zero when it is down.
    pub raised_ticks: u16,
    /// Hit points the raised barrier can still absorb; zero when it is down.
    pub absorb_left: SimScalar,
    /// Ticks until the barrier can be raised again; zero when it is ready.
    pub cooldown_ticks: u16,
}

impl BarrierState {
    /// Absorbs what it can of `damage` (hit points, before armour) and returns the rest.
    ///
    /// Without a `spec`, or with nothing to absorb, `damage` passes unchanged. A barrier that
    /// is down and ready is raised by this damage first: `StarCraft` II's `BarrierDamageResponse`
    /// (`voidmulti` BehaviorData:577-590, cooldown 45) applies the `TakenDamage` window
    /// (BehaviorData:599-609, 3 Normal s) that enables the absorbing `ImmortalOverload`
    /// response (`void` BehaviorData:97-108, fraction 0; `voidmulti` :273-279, limit 100). A
    /// barrier that has absorbed its limit is spent: it goes down at once and stays down until
    /// its cooldown ends.
    pub fn absorb(&mut self, spec: Option<&BarrierSpec>, damage: SimScalar) -> SimScalar {
        let Some(spec) = spec else {
            return damage;
        };
        if damage <= SimScalar::ZERO {
            return damage;
        }
        if self.raised_ticks == 0 {
            if self.cooldown_ticks > 0 {
                return damage;
            }
            self.raised_ticks = spec.duration_ticks;
            self.absorb_left = spec.absorb_limit;
            self.cooldown_ticks = spec.cooldown_ticks;
        }
        let absorbed = damage.min(self.absorb_left);
        self.absorb_left -= absorbed;
        if self.absorb_left <= SimScalar::ZERO {
            self.lower();
        }
        damage - absorbed
    }

    /// Advances the barrier by one tick: a raised barrier runs down its window and the
    /// cooldown counts towards readiness.
    pub fn tick(&mut self) {
        self.cooldown_ticks = self.cooldown_ticks.saturating_sub(1);
        if self.raised_ticks > 0 {
            self.raised_ticks -= 1;
            if self.raised_ticks == 0 {
                self.lower();
            }
        }
    }

    /// Whether the barrier is up and absorbing.
    #[must_use]
    pub const fn is_raised(&self) -> bool {
        self.raised_ticks > 0
    }

    fn lower(&mut self) {
        self.raised_ticks = 0;
        self.absorb_left = SimScalar::ZERO;
    }
}

/// Authoritative simulation entity state for a single unit or building dome.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Entity {
    /// Unique simulation identifier.
    pub id: EntityId,
    /// Archetype identifier.
    pub unit_type_id: UnitTypeId,
    /// Owning player slot.
    pub owner: PlayerId,
    /// 2D planar position in continuous world space.
    pub position: SimVec2,
    /// Authoritative position at the start of the current simulation tick.
    pub pos_at_tick_start: SimVec2,
    /// During the tick, the velocity movement intends; afterward, the displacement this unit
    /// actually achieved. A unit pressing against a wall therefore reads as stationary.
    pub velocity: SimVec2,
    /// Hull facing: a binary angle (65 536 units per turn), counter-clockwise from +x in the
    /// world frame. Set at spawn (`SimWorld::spawn_unit_facing`) and afterwards changed only
    /// through [`Self::turn_hull_towards`], so it turns at most the archetype's `turn_rate` on a
    /// tick the unit moves and its `stationary_turn_rate` on any other tick. For an archetype flagged `turn_before_move`, the movement phase holds the unit in
    /// place while this facing is further from its step than one tick's turn, and lets it step
    /// only on a tick that ends facing the step exactly.
    pub heading: SimAngle,
    /// Independent turret aim angle.
    pub turret_angle: SimAngle,
    /// Current hull hit points.
    pub hp: SimScalar,
    /// Current plasma shield energy.
    pub shields: SimScalar,
    /// Current caster energy pool.
    pub energy: SimScalar,
    /// Ticks elapsed since last sustaining damage, to shields or life, saturating at
    /// `u16::MAX`. Life regenerates once it reaches the archetype's `hp_regen_delay_ticks`, and
    /// shields once it reaches `shield_regen_delay_ticks`. A new entity starts at `u16::MAX`
    /// ("never damaged"), which is ready for any authored delay.
    pub ticks_since_last_damage: u16,
    /// `StarCraft` II style order queue.
    pub order_queue: VecDeque<UnitOrder>,
    /// Simple Stupid Funnel string-pulled waypoints along `NavMesh`.
    pub current_funnel_waypoints: Vec<SimVec2>,
    /// Movement order progress carried between ticks.
    pub movement: MovementMemory,
    /// Weapon state machine phase.
    pub weapon_phase: WeaponPhase,
    /// Remaining ticks until weapon cooldown expires.
    pub weapon_cooldown_remaining: u16,
    /// The fraction of a tick of random firing delay drawn but not yet charged, in `0..1`
    /// ticks: each release adds its delay here and moves the whole ticks into the cooldown, so
    /// the shot times are the exact sums of periods and delays quantised to ticks (B-3).
    pub weapon_delay_carry: SimScalar,
    /// Strikes of the last attack still to land on later ticks; `None` when its every strike
    /// has landed or the weapon lands all of them on the release tick.
    pub pending_strikes: Option<PendingStrikes>,
    /// Active temporary stat buffs and debuffs.
    pub active_buffs: Vec<ActiveBuff>,
    /// The damage-absorbing barrier's window, remaining absorption and cooldown (B-41).
    pub barrier: BarrierState,
    /// FIFO production jobs: archetype, remaining 25 Hz updates and reserved supply.
    /// Only the front job advances per update, preserving the authored `u32` duration.
    pub production_queue: VecDeque<crate::production::ProductionJob>,
    /// Supply consumption captured at spawn; released only when this entity is reaped.
    pub captured_supply_used: u32,
    /// Completed supply provision captured at spawn, independent of later catalog edits.
    pub captured_supply_provided: u32,
    /// Task order progress carried between ticks.
    pub task: UnitTask,
    /// Resources this harvester holds; survives order changes until delivered.
    pub cargo: Option<Cargo>,
    /// Construction record while this building is an unfinished foundation.
    pub construction: Option<crate::production::construction::Construction>,
    /// Mineral walking flag allowing pass-through of unit collisions.
    pub is_mineral_walking: bool,
    /// Flag indicating whether the unit is anchored in Hold Position.
    pub is_holding_position: bool,
    /// High-level animation state for renderer synchronization.
    pub current_anim: AnimState,
    /// Direction the last hull hit originated from.
    pub last_hit_angle: SimAngle,
    /// Ticks remaining on shield impact visual ripple flash.
    pub shield_hit_flash_ticks: u8,
    /// Ticks remaining on hull impact visual flare flash.
    pub hull_hit_flash_ticks: u8,
    /// Currently acquired combat target handle (used for independent turret tracking).
    pub current_target: Option<EntityId>,
    /// Where this unit's automatic target last stood, world units, recorded by target
    /// acquisition for an archetype whose tie-break is `AcquireTieBreak::ClosestToPrevious`, so
    /// that once the target is lost (dead, gone, out of reach) the unit takes the candidate
    /// closest to it. `None` until such a unit first holds a target.
    pub previous_target_position: Option<SimVec2>,
    /// The target this unit's weapon is engaged with: set when a swing starts against it, and
    /// cleared when the weapon works against another target or none, or when the target leaves
    /// the weapon's engaged arc. A target the weapon is engaged with is held within the engaged
    /// arc (`StarCraft` II `Arc` ÷ 2 + `ArcSlop`); any other must first come within the
    /// acquisition arc (`Arc` ÷ 2) (plan `2026-11-05-sc2-deviation-fixes` WP10, B-29).
    pub engaged_target: Option<EntityId>,
    /// Whether the unit is completely immobile (e.g. Siege Mode, Burrowed, Channeling).
    pub is_immobile: bool,
    /// Collision unpushability flag (cannot be displaced by friendly push priority).
    pub unpushable: bool,
    /// Active ability / siege mode state.
    pub is_sieged: bool,
    /// Active siege mode weapon configuration.
    pub siege_weapon: Option<WeaponSpec>,
    /// Active generalized stance / mode identifier (e.g. "`siege_mode`", "burrowed").
    pub active_mode: Option<String>,
    /// Remaining ticks of channeled casting / transformation.
    pub channeling_remaining_ticks: u16,
    /// Pending ability queued or being channeled: (`ability_index`, `target_pos`, `target_entity`).
    pub pending_ability: Option<(u8, Option<SimVec2>, Option<EntityId>)>,
    /// Independent ability cooldown timers for up to 4 ability slots.
    pub ability_cooldowns: [u16; 4],
    /// Last tick at which this unit sustained damage from an active hazard zone (Psionic Storm non-stacking).
    pub last_storm_damage_tick: Option<Tick>,
    /// Cloaking / subterranean invisibility flag.
    pub is_cloaked: bool,
    /// Maximum hull hit points including upgrades.
    pub max_hp: SimScalar,
    /// Whether this building or unit has completed construction.
    pub is_completed: bool,
    /// Sole carrier identity while absent from world-space participation.
    /// Only validated world transactions and controlled reap may change this link.
    pub(crate) transport_carrier: Option<EntityId>,
    /// Invulnerability flag (e.g. Stasis field, Phase Shift, Recall, Divine Shield).
    pub is_invulnerable: bool,
    /// Hull rotation still allowed this tick, in binary angle units.
    ///
    /// Set to the archetype's `stationary_turn_rate` at the start of every movement phase
    /// (`SimWorld::step_movement_and_steering`), raised or lowered to its `turn_rate` when the
    /// unit's own motion translates it that tick, and spent by [`Self::turn_hull_towards`]. So the
    /// movement phase and the weapon phase together turn the hull at most `turn_rate` on a tick
    /// the unit moves and `stationary_turn_rate` on any other tick. A `turn_before_move` unit
    /// turning in place spends all of it in the movement phase.
    /// It is reset before its first read in every tick and so carries no state from one tick
    /// to the next; for that reason it is left out of the state checksum.
    pub hull_turn_budget: u16,
}

impl Entity {
    /// Turns the hull towards `desired` by at most this tick's remaining budget, spends the
    /// rotation used, and returns the new heading.
    ///
    /// The turn takes the shorter way round; a facing within the budget is reached exactly.
    /// Both angles are binary angles in the world frame.
    pub fn turn_hull_towards(&mut self, desired: SimAngle) -> SimAngle {
        let before = self.heading;
        self.heading = before.rotate_towards(desired, self.hull_turn_budget);
        let used = before.diff(self.heading).unsigned_abs();
        self.hull_turn_budget = self.hull_turn_budget.saturating_sub(used);
        self.heading
    }

    /// Returns true if this entity cannot be pushed aside by friendly units.
    #[must_use]
    #[inline]
    pub fn cannot_be_pushed(&self) -> bool {
        self.is_on_hold() || self.unpushable || self.is_immobile
    }

    /// Creates a new entity initialized from its archetype definition.
    #[must_use]
    pub fn from_archetype(
        id: EntityId,
        owner: PlayerId,
        position: SimVec2,
        archetype: &UnitArchetype,
    ) -> Self {
        let is_immobile_archetype = matches!(archetype.shape, ShapeKind::BuildingDome { .. })
            || archetype.max_speed <= SimScalar::ZERO;

        Self {
            id,
            unit_type_id: archetype.unit_type_id,
            owner,
            position,
            pos_at_tick_start: position,
            velocity: SimVec2::ZERO,
            heading: SimAngle::ZERO,
            turret_angle: SimAngle::ZERO,
            hp: archetype.max_hp,
            shields: archetype.max_shields,
            energy: archetype.start_energy,
            // Never damaged: ready to regenerate shields under any authored delay, and the
            // value the saturating counter reaches anyway.
            ticks_since_last_damage: u16::MAX,
            order_queue: VecDeque::new(),
            current_funnel_waypoints: Vec::new(),
            movement: MovementMemory {
                // Half of any group goes each way round, without needing to confer.
                flank_left: id.0.is_multiple_of(2),
                ..MovementMemory::default()
            },
            weapon_phase: WeaponPhase::Ready,
            weapon_cooldown_remaining: 0,
            weapon_delay_carry: SimScalar::ZERO,
            barrier: BarrierState::default(),
            pending_strikes: None,
            active_buffs: Vec::new(),
            production_queue: VecDeque::new(),
            captured_supply_used: archetype.supply_cost,
            captured_supply_provided: archetype.supply_provided,
            task: UnitTask::default(),
            cargo: None,
            construction: None,
            is_mineral_walking: false,
            is_holding_position: false,
            current_anim: AnimState::Idle,
            last_hit_angle: SimAngle::ZERO,
            shield_hit_flash_ticks: 0,
            hull_hit_flash_ticks: 0,
            current_target: None,
            previous_target_position: None,
            engaged_target: None,
            is_immobile: is_immobile_archetype,
            unpushable: is_immobile_archetype,
            is_sieged: false,
            siege_weapon: None,
            active_mode: None,
            channeling_remaining_ticks: 0,
            pending_ability: None,
            ability_cooldowns: [0; 4],
            last_storm_damage_tick: None,
            is_cloaked: false,
            max_hp: archetype.max_hp,
            is_completed: true,
            transport_carrier: None,
            is_invulnerable: false,
            hull_turn_budget: archetype.turn_rate.raw(),
        }
    }

    /// Returns the carrier identity while this actor is absent from world-space participation.
    /// The owning world validates membership, owner and lifecycle consistency.
    #[must_use]
    pub const fn transport_carrier(&self) -> Option<EntityId> {
        self.transport_carrier
    }

    /// Whether this actor is carried; derived solely from its authoritative carrier link.
    #[must_use]
    pub const fn is_loaded_in_transport(&self) -> bool {
        self.transport_carrier.is_some()
    }

    /// Whether the front order is a task, which [`crate::economy`] drives instead of movement.
    #[must_use]
    pub fn has_task_order(&self) -> bool {
        matches!(
            self.order_queue.front(),
            Some(
                UnitOrder::Harvest { .. }
                    | UnitOrder::ReturnCargo
                    | UnitOrder::Build { .. }
                    | UnitOrder::Construct { .. }
                    | UnitOrder::Load { .. }
                    | UnitOrder::UnloadAll
            )
        )
    }

    /// Drops everything tied to the current task leg; what outlives an order stays.
    pub(crate) fn abandon_task_leg(&mut self) {
        self.task = crate::economy::fresh_leg(self.task);
        self.is_mineral_walking = false;
    }

    /// Handles player move/stop input during combat, executing SC2 backswing cancellation.
    fn handle_interrupting_move(&mut self, move_pos: SimVec2) {
        match self.weapon_phase {
            WeaponPhase::Windup { .. } => {
                // ABORT: Shot was winding up; moving cancels the shot with zero damage.
                self.weapon_phase = WeaponPhase::Ready;
                self.order_queue.clear();
                self.order_queue
                    .push_back(UnitOrder::Move { target: move_pos });
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
            WeaponPhase::Backswing { .. } => {
                // CANCEL BACKSWING: Damage has already been applied!
                // Terminate the backswing animation immediately.
                // The weapon cooldown continues to tick down in the background.
                self.weapon_phase = WeaponPhase::Cooldown;
                self.order_queue.clear();
                self.order_queue
                    .push_back(UnitOrder::Move { target: move_pos });
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
            _ => {
                self.order_queue.clear();
                self.order_queue
                    .push_back(UnitOrder::Move { target: move_pos });
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
        }
    }

    /// Issues an order, or schedules it when `queue` is true.
    /// Returns false for a loaded actor. An issued attack is explicit by definition, whatever
    /// origin the command claimed.
    ///
    /// # Errors
    /// [`EntityAdmissionError::StimOutsideWorld`] for an immediate `UseStim`, before changing any
    /// field: a stim comes from the catalog, through `SimWorld`'s command path.
    pub fn issue_order(
        &mut self,
        mut order: UnitOrder,
        queue: bool,
    ) -> Result<bool, EntityAdmissionError> {
        if self.is_loaded_in_transport() {
            return Ok(false);
        }
        if let UnitOrder::AttackTarget { origin, .. } = &mut order {
            *origin = AttackOrigin::Explicit;
        }
        if !queue && matches!(order, UnitOrder::UseStim) {
            return Err(EntityAdmissionError::StimOutsideWorld);
        }
        self.commit_order(order, queue);
        Ok(true)
    }

    /// Infallible concrete movement commit after world selection preflight.
    pub(crate) fn commit_movement_order(&mut self, target: SimVec2, attack: bool, queue: bool) {
        let order = if attack {
            UnitOrder::AttackMove { target }
        } else {
            UnitOrder::Move { target }
        };
        self.commit_order(order, queue);
    }

    // An immediate UseStim is refused by the public wrapper; the concrete movement
    // helper only constructs Move/AttackMove. No fallible work follows admission.
    fn commit_order(&mut self, order: UnitOrder, queue: bool) {
        if queue {
            self.order_queue.push_back(order);
            return;
        }

        self.movement.restart();
        self.abandon_task_leg();
        match order {
            UnitOrder::Move { target } => {
                self.handle_interrupting_move(target);
            }
            UnitOrder::HoldPosition => {
                self.is_holding_position = true;
                self.velocity = SimVec2::ZERO;
                self.order_queue.clear();
                self.order_queue.push_back(UnitOrder::HoldPosition);
                self.current_funnel_waypoints.clear();
            }
            UnitOrder::Stop => {
                if matches!(self.weapon_phase, WeaponPhase::Windup { .. }) {
                    self.weapon_phase = WeaponPhase::Ready;
                } else if matches!(self.weapon_phase, WeaponPhase::Backswing { .. }) {
                    self.weapon_phase = WeaponPhase::Cooldown;
                }
                self.velocity = SimVec2::ZERO;
                self.order_queue.clear();
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
            UnitOrder::AttackTarget { target, origin } => {
                if matches!(self.weapon_phase, WeaponPhase::Windup { .. }) {
                    self.weapon_phase = WeaponPhase::Ready;
                }
                self.current_target = Some(target);
                self.order_queue.clear();
                self.order_queue
                    .push_back(UnitOrder::AttackTarget { target, origin });
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
            _ => {
                if matches!(self.weapon_phase, WeaponPhase::Windup { .. }) {
                    self.weapon_phase = WeaponPhase::Ready;
                }
                self.order_queue.clear();
                self.order_queue.push_back(order);
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
        }
    }

    /// Applies authored Overclock after preparing HP and the complete ordered speed fold.
    /// Returns false, changing nothing, when life does not exceed the cost
    /// ([`Self::can_pay_overclock`]); errors leave all fields unchanged.
    ///
    /// # Errors
    /// Rejects malformed rates/costs or unrepresentable payments/folds.
    pub fn apply_overclock(&mut self, spec: OverclockSpec) -> Result<bool, EntityAdmissionError> {
        let Some(prepared) = self.prepare_overclock(spec)? else {
            return Ok(false);
        };
        self.commit_overclock(prepared);
        Ok(true)
    }

    /// Whether this entity has enough life to pay `spec`'s life cost.
    ///
    /// Overclock (Stimpack) can never kill its caster, so it is admitted only when the current
    /// life (`hp`, in hit points) is strictly greater than `spec.hp_cost`; at or below the cost
    /// it is refused and changes nothing (owner answer to Q0053, bug B-33). The payment then
    /// always leaves positive life. Every admission path, and the client's command filter, asks
    /// this one predicate.
    #[must_use]
    pub fn can_pay_overclock(&self, spec: &OverclockSpec) -> bool {
        self.hp > spec.hp_cost
    }

    pub(crate) fn prepare_overclock(
        &self,
        spec: OverclockSpec,
    ) -> Result<Option<PreparedOverclock>, EntityAdmissionError> {
        AttackRate::new(spec.attack_rate_multiplier)?;
        if spec.hp_cost < SimScalar::ZERO {
            return Err(EntityAdmissionError::NegativeCost);
        }
        let multiplier = self.prospective_speed_multiplier(1, spec.speed_multiplier)?;
        if !self.can_pay_overclock(&spec) {
            return Ok(None);
        }
        let hp = self
            .hp
            .checked_sub(spec.hp_cost)
            .ok_or(EntityAdmissionError::PaymentOverflow)?;
        Ok(Some(PreparedOverclock {
            spec,
            hp,
            multiplier,
        }))
    }

    pub(crate) fn commit_overclock(&mut self, prepared: PreparedOverclock) {
        let spec = prepared.spec;
        self.hp = prepared.hp;
        if let Some(buff) = self.active_buffs.iter_mut().find(|b| b.buff_id == 1) {
            buff.remaining_ticks = spec.duration_ticks;
            buff.speed_multiplier = spec.speed_multiplier;
            buff.attack_rate_multiplier = spec.attack_rate_multiplier;
        } else {
            self.active_buffs.push(ActiveBuff {
                buff_id: 1,
                remaining_ticks: spec.duration_ticks,
                speed_multiplier: spec.speed_multiplier,
                attack_rate_multiplier: spec.attack_rate_multiplier,
            });
        }
    }

    /// Applies the existing half-speed75-tick debuff after checked fold preparation.
    ///
    /// # Errors
    /// Rejects malformed or unrepresentable buff state without changing duration.
    pub fn apply_concussive_slow(&mut self) -> Result<(), EntityAdmissionError> {
        self.speed_multiplier()?;
        if let Some(buff) = self.active_buffs.iter_mut().find(|b| b.buff_id == 2) {
            buff.remaining_ticks = 75;
        } else {
            self.prospective_speed_multiplier(2, SimScalar::HALF)?;
            self.active_buffs.push(ActiveBuff {
                buff_id: 2,
                remaining_ticks: 75,
                speed_multiplier: SimScalar::HALF,
                attack_rate_multiplier: SimScalar::ONE,
            });
        }
        Ok(())
    }

    fn prospective_speed_multiplier(
        &self,
        id: u8,
        replacement: SimScalar,
    ) -> Result<SimScalar, MovementMathError> {
        self.speed_multiplier()?;
        let mut replaced = false;
        let mut product = SimScalar::ONE;
        for (index, buff) in self.active_buffs.iter().enumerate() {
            let value = if !replaced && buff.buff_id == id {
                replaced = true;
                replacement
            } else {
                buff.speed_multiplier
            };
            product = fold_speed(product, value, index)?;
        }
        if !replaced {
            product = fold_speed(product, replacement, self.active_buffs.len())?;
        }
        Ok(product)
    }

    /// Applies calculated damage from an incoming attack.
    pub fn apply_damage(
        &mut self,
        shield_damage: SimScalar,
        life_damage: SimScalar,
        hit_angle: SimAngle,
    ) {
        self.shields = (self.shields - shield_damage).max(SimScalar::ZERO);
        self.hp = (self.hp - life_damage).max(SimScalar::ZERO);
        self.ticks_since_last_damage = 0;
        self.last_hit_angle = hit_angle;
        if shield_damage > SimScalar::ZERO {
            self.shield_hit_flash_ticks = 8;
        }
        if life_damage > SimScalar::ZERO {
            self.hull_hit_flash_ticks = 8;
        }
    }

    /// Completes the stance change an ability effect describes, however it was started.
    ///
    /// Instant casts, finished channels and energy exhaustion all end here, so a stance
    /// means the same thing on every path. Other effects change nothing.
    pub(crate) fn apply_mode_transition(
        &mut self,
        arch: &UnitArchetype,
        effect: &AbilityEffectSpec,
    ) {
        match effect {
            AbilityEffectSpec::SwitchMode { mode_name } => {
                let Some(spec) = arch.get_mode(mode_name) else {
                    return;
                };
                if self.active_mode.as_deref() == Some(mode_name.as_str()) {
                    self.leave_mode(arch);
                } else {
                    self.active_mode = Some(mode_name.clone());
                    self.is_immobile = spec.is_immobile;
                    self.unpushable = spec.unpushable;
                    self.is_cloaked = spec.is_cloaked;
                    self.siege_weapon = spec.ground_weapon_override.clone().flatten();
                    self.is_sieged = self.siege_weapon.is_some();
                }
            }
            AbilityEffectSpec::SiegeModeToggle { sieged_weapon, .. } => {
                if self.is_sieged {
                    self.leave_mode(arch);
                } else {
                    self.active_mode = Some(SIEGE_MODE_NAME.to_owned());
                    self.is_sieged = true;
                    self.is_immobile = true;
                    self.unpushable = true;
                    self.siege_weapon = Some(WeaponSpec::clone(sieged_weapon));
                }
            }
            _ => {}
        }
    }

    /// Starts the channel that ends in a stance change; the unit is anchored meanwhile.
    pub(crate) fn begin_mode_channel(
        &mut self,
        cast_ticks: u16,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) {
        self.is_immobile = true;
        self.unpushable = true;
        self.channeling_remaining_ticks = cast_ticks;
        self.pending_ability = Some((ability_index, target_pos, target_entity));
    }

    /// Returns to the archetype's default stance.
    pub(crate) fn leave_mode(&mut self, arch: &UnitArchetype) {
        let is_immobile_default = arch.is_immobile_by_default();
        self.active_mode = None;
        self.is_immobile = is_immobile_default;
        self.unpushable = is_immobile_default;
        self.is_cloaked = false;
        self.siege_weapon = None;
        self.is_sieged = false;
    }

    /// The sieged weapon replacing the archetype's ground weapon, if one is active.
    #[must_use]
    #[inline]
    pub fn active_siege_weapon(&self) -> Option<&WeaponSpec> {
        self.is_sieged
            .then_some(self.siege_weapon.as_ref())
            .flatten()
    }

    /// Whether the front order keeps this unit anchored in place.
    #[must_use]
    #[inline]
    pub fn is_on_hold(&self) -> bool {
        self.is_holding_position
            || matches!(self.order_queue.front(), Some(UnitOrder::HoldPosition))
    }

    /// Whether this entity is alive (`hp > 0`).
    #[must_use]
    #[inline]
    pub fn is_alive(&self) -> bool {
        self.hp > SimScalar::ZERO
    }

    /// Returns the ordered nonnegative Q32.32 movement multiplier without saturation.
    /// Quantization may underflow to zero, which is a valid stationary rate.
    ///
    /// # Errors
    /// Identifies a negative multiplier or an unrepresentable ordered product.
    pub fn speed_multiplier(&self) -> Result<SimScalar, MovementMathError> {
        self.active_buffs
            .iter()
            .enumerate()
            .try_fold(SimScalar::ONE, |product, (index, buff)| {
                fold_speed(product, buff.speed_multiplier, index)
            })
    }

    /// Returns the ordered, representationally saturated positive buff-rate product.
    ///
    /// # Errors
    /// Returns exact buff evidence if direct mutation introduced a nonpositive rate.
    pub fn attack_rate_multiplier(&self) -> Result<AttackRate, InvalidBuffAttackRate> {
        combined_attack_rate(&self.active_buffs)
    }
}

fn fold_speed(
    product: SimScalar,
    value: SimScalar,
    index: usize,
) -> Result<SimScalar, MovementMathError> {
    if value < SimScalar::ZERO {
        return Err(MovementMathError::NegativeMultiplier { index });
    }
    product
        .checked_mul(value)
        .ok_or(MovementMathError::Unrepresentable)
}

#[cfg(test)]
mod barrier_state_tests {
    use super::BarrierState;
    use sim_types::{BarrierSpec, SimScalar};

    fn spec() -> BarrierSpec {
        BarrierSpec {
            absorb_limit: SimScalar::from_num(100),
            duration_ticks: 3,
            cooldown_ticks: 5,
        }
    }

    fn n(value: i32) -> SimScalar {
        SimScalar::from_num(value)
    }

    #[test]
    fn without_a_spec_damage_passes_and_nothing_is_raised() {
        let mut barrier = BarrierState::default();
        assert_eq!(barrier.absorb(None, n(40)), n(40));
        assert_eq!(barrier, BarrierState::default());
    }

    #[test]
    fn no_damage_leaves_a_ready_barrier_down() {
        let mut barrier = BarrierState::default();
        assert_eq!(
            barrier.absorb(Some(&spec()), SimScalar::ZERO),
            SimScalar::ZERO
        );
        assert!(!barrier.is_raised());
        assert_eq!(barrier.cooldown_ticks, 0);
    }

    #[test]
    fn the_raising_damage_is_absorbed_and_the_limit_spends_it() {
        let mut barrier = BarrierState::default();
        assert_eq!(barrier.absorb(Some(&spec()), n(70)), SimScalar::ZERO);
        assert!(barrier.is_raised());
        assert_eq!(barrier.absorb_left, n(30));
        assert_eq!(barrier.absorb(Some(&spec()), n(45)), n(15));
        assert!(!barrier.is_raised(), "spent at its limit");
        assert_eq!(
            barrier.absorb(Some(&spec()), n(10)),
            n(10),
            "down while cooling"
        );
    }

    #[test]
    fn the_window_counts_the_raising_tick_and_the_cooldown_rearms() {
        let mut barrier = BarrierState::default();
        barrier.absorb(Some(&spec()), n(1));
        barrier.tick();
        barrier.tick();
        assert!(barrier.is_raised(), "up through its third tick");
        barrier.tick();
        assert!(!barrier.is_raised());
        assert_eq!(barrier.absorb_left, SimScalar::ZERO);
        assert_eq!(barrier.absorb(Some(&spec()), n(1)), n(1));
        barrier.tick();
        barrier.tick();
        assert_eq!(barrier.cooldown_ticks, 0);
        assert_eq!(
            barrier.absorb(Some(&spec()), n(1)),
            SimScalar::ZERO,
            "raised again"
        );
    }
}
