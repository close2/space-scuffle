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
    AnimState, EntityId, MoverType, OverclockSpec, PlayerId, ShapeKind, SimAngle, SimScalar,
    SimVec2, Tick, UnitOrder, UnitTypeId,
};
use std::collections::VecDeque;

// PERFORMANCE RATIONALE:
// Why: Standard HashMap iteration is non-deterministic across executions, causing lockstep desyncs.
// Naive sorting of keys on every tick incurs an O(N log N) overhead per query.
// What: Authoritative entities are stored in a sorted `BTreeMap<EntityId, Entity>` with deterministic
// traversal by EntityId.
// Trade-off: BTreeMap has slightly higher pointer-chase overhead compared to contiguous Vecs,
// but guarantees strict, deterministic, allocation-free in-order iteration for checksums and systems.

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
    Windup {
        /// Target entity handle.
        target_id: EntityId,
        /// Remaining ticks before shot discharge.
        remaining_ticks: u16,
    },
    /// Damage has been applied; unit is executing the post-shot backswing animation.
    /// Can be interrupted and cancelled by a Move command without DPS loss.
    Backswing {
        /// Target entity handle.
        target_id: EntityId,
        /// Remaining ticks in the animation follow-through.
        remaining_ticks: u16,
    },
    /// Weapon is on cooldown following an attack. Unit can move and perform other actions.
    Cooldown,
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
    /// 2D linear velocity per tick.
    pub velocity: SimVec2,
    /// Hull facing angle.
    pub heading: SimAngle,
    /// Independent turret aim angle.
    pub turret_angle: SimAngle,
    /// Current hull hit points.
    pub hp: SimScalar,
    /// Current plasma shield energy.
    pub shields: SimScalar,
    /// Current caster energy pool.
    pub energy: SimScalar,
    /// Ticks elapsed since last sustaining damage (for 10s / 250-tick shield regen).
    pub ticks_since_last_damage: u16,
    /// `StarCraft` II style order queue.
    pub order_queue: VecDeque<UnitOrder>,
    /// Simple Stupid Funnel string-pulled waypoints along `NavMesh`.
    pub current_funnel_waypoints: Vec<SimVec2>,
    /// Weapon state machine phase.
    pub weapon_phase: WeaponPhase,
    /// Remaining ticks until weapon cooldown expires.
    pub weapon_cooldown_remaining: u16,
    /// Active temporary stat buffs and debuffs.
    pub active_buffs: Vec<ActiveBuff>,
    /// FIFO production jobs: archetype, remaining 25 Hz updates and reserved supply.
    /// Only the front job advances per update, preserving the authored `u32` duration.
    pub production_queue: VecDeque<crate::production::ProductionJob>,
    /// Supply consumption captured at spawn; released only when this entity is reaped.
    pub captured_supply_used: u32,
    /// Completed supply provision captured at spawn, independent of later catalog edits.
    pub captured_supply_provided: u32,
    /// Mineral walking flag allowing pass-through of unit collisions.
    pub is_mineral_walking: bool,
    /// Flag indicating whether the unit is anchored in Hold Position.
    pub is_holding_position: bool,
    /// High-level animation state for renderer synchronization.
    pub current_anim: AnimState,
    /// Normalized animation progress within loop.
    pub anim_progress: SimScalar,
    /// Direction the last hull hit originated from.
    pub last_hit_angle: SimAngle,
    /// Ticks remaining on shield impact visual ripple flash.
    pub shield_hit_flash_ticks: u8,
    /// Ticks remaining on hull impact visual flare flash.
    pub hull_hit_flash_ticks: u8,
    /// Currently acquired combat target handle (used for independent turret tracking).
    pub current_target: Option<EntityId>,
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
}

impl Entity {
    /// Returns true if this entity cannot be pushed aside by friendly units.
    #[must_use]
    #[inline]
    pub fn cannot_be_pushed(&self) -> bool {
        self.is_holding_position || self.unpushable || self.is_immobile
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
            energy: archetype.max_energy,
            ticks_since_last_damage: 250, // Starts ready for shield regen
            order_queue: VecDeque::new(),
            current_funnel_waypoints: Vec::new(),
            weapon_phase: WeaponPhase::Ready,
            weapon_cooldown_remaining: 0,
            active_buffs: Vec::new(),
            production_queue: VecDeque::new(),
            captured_supply_used: archetype.supply_cost,
            captured_supply_provided: archetype.supply_provided,
            is_mineral_walking: false,
            is_holding_position: false,
            current_anim: AnimState::Idle,
            anim_progress: SimScalar::ZERO,
            last_hit_angle: SimAngle::ZERO,
            shield_hit_flash_ticks: 0,
            hull_hit_flash_ticks: 0,
            current_target: None,
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
    /// Returns false for a loaded actor or ordinary immediate legacy-stim HP ineligibility.
    ///
    /// # Errors
    /// Returns typed malformed buff/cost evidence before changing any field.
    pub fn issue_order(
        &mut self,
        order: UnitOrder,
        queue: bool,
    ) -> Result<bool, EntityAdmissionError> {
        if self.is_loaded_in_transport() {
            return Ok(false);
        }
        if !queue && matches!(order, UnitOrder::UseStim) {
            return self.apply_stimpack();
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

    // Immediate UseStim is admitted by the public wrapper; the concrete movement
    // helper only constructs Move/AttackMove. No fallible work follows admission.
    fn commit_order(&mut self, order: UnitOrder, queue: bool) {
        if queue {
            self.order_queue.push_back(order);
            return;
        }

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
            UnitOrder::AttackTarget { target } => {
                if matches!(self.weapon_phase, WeaponPhase::Windup { .. }) {
                    self.weapon_phase = WeaponPhase::Ready;
                }
                self.current_target = Some(target);
                self.order_queue.clear();
                self.order_queue
                    .push_back(UnitOrder::AttackTarget { target });
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
    /// Returns false for ordinary low-HP eligibility; errors leave all fields unchanged.
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

    pub(crate) fn prepare_overclock(
        &self,
        spec: OverclockSpec,
    ) -> Result<Option<PreparedOverclock>, EntityAdmissionError> {
        AttackRate::new(spec.attack_rate_multiplier)?;
        if spec.hp_cost < SimScalar::ZERO {
            return Err(EntityAdmissionError::NegativeCost);
        }
        let multiplier = self.prospective_speed_multiplier(1, spec.speed_multiplier)?;
        if self.hp <= SimScalar::ONE {
            return Ok(None);
        }
        let hp = self
            .hp
            .checked_sub(spec.hp_cost)
            .ok_or(EntityAdmissionError::PaymentOverflow)?
            .max(SimScalar::ONE);
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

    /// Legacy default Overclock admission; parameters retain existing gameplay values.
    ///
    /// # Errors
    /// Propagates malformed buff state/payment evidence without mutation.
    pub fn apply_stimpack(&mut self) -> Result<bool, EntityAdmissionError> {
        self.apply_overclock(OverclockSpec {
            hp_cost: SimScalar::from_i32(10),
            duration_ticks: 250,
            speed_multiplier: SimScalar::from_num(1.5),
            attack_rate_multiplier: SimScalar::from_num(1.5),
        })
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
