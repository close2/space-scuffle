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
    AbilityEffectSpec, AnimState, AttackCommand, AttackDashSpec, AttackOrigin, BarrierSpec,
    EffectSource, EntityId, FollowKind, MoverType, OverclockSpec, PerPlayer, PlayerId,
    ResourceKind, ResourceNodeId, ShapeKind, SimAngle, SimScalar, SimVec2, StanceOrders,
    UnitModeSpec, UnitOrder, UnitTypeId,
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

/// [`ActiveBuff::buff_id`] of a running attack dash ([`AttackDashSpec`], `StarCraft` II's
/// `Charging` buff, B-43).
pub const ATTACK_DASH_BUFF_ID: u8 = 3;

/// [`ActiveBuff::buff_id`] of a running overclock ([`OverclockSpec`], `StarCraft` II's Stimpack).
pub const OVERCLOCK_BUFF_ID: u8 = 1;

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

/// The searches of a spaced line area that land after its release tick (plan
/// `2026-11-05-sc2-deviation-fixes` Amendment 24, B-85).
///
/// In `StarCraft` II the Lurker's spines are a `CEffectCreatePersistent` whose periods search
/// one point of the line each, 0.125 Normal s apart (`void` EffectData:297-307), while the
/// Lurker is alive and burrowed (`PeriodicValidator` `CasterIsAliveandBurrowedLurker`,
/// `voidmulti` EffectData:813). Combat resolution lands the first search on the release tick and
/// keeps the rest here, one every [`sim_types::SplashLine::period_ticks`]. The points are fixed
/// where the release laid them out; a unit standing in a search when it lands is struck, unless
/// an earlier search of the same line struck it (the game's `noMarkers`). The attacker's death,
/// boarding a transport, a mode change starting (channeling, such as unburrowing) or losing the
/// weapon ends the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingLine {
    /// Centres of the searches still to land, world units, the next first.
    pub points: Vec<SimVec2>,
    /// Ticks until the next search lands, at least 1 between ticks.
    pub ticks_until_next: u16,
    /// Combat plane of the struck target: the weapon is resolved against it again for each
    /// search, and only units on it are struck.
    pub plane: MoverType,
    /// Direction the attack came from, for the struck units' hit feedback.
    pub angle: SimAngle,
    /// The attack's target, struck directly on the release tick and spared by the searches.
    pub primary: EntityId,
    /// Units an earlier search of this line struck, in the order they were struck.
    pub struck: Vec<EntityId>,
    /// What the renderer is told about each search as it lands; presentation only, not hashed.
    pub view: PendingLineView,
}

/// The presentation of a [`PendingLine`]'s searches (plan `2026-11-09-effect-visualization`):
/// each search, when it lands, is published as a `SplashVisual` numbered by its place in the
/// whole line, as the searches of an instant line are.
///
/// Not hashed: no simulation decision reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingLineView {
    /// The attacker's type and weapon that released the line.
    pub effect: EffectSource,
    /// The whole line in world units: the point its offsets are measured from, that point moved
    /// by the largest offset, and the spacing of its first two offsets.
    pub line: Option<(SimVec2, SimVec2, SimScalar)>,
    /// How many searches the whole line has, the first included.
    pub count: u8,
}

/// The strikes of an attack that land after its release tick (plan
/// `2026-11-05-sc2-deviation-fixes` WP5, B-2).
///
/// In `StarCraft` II a multi-strike weapon is a `CEffectCreatePersistent` whose periods land one
/// strike each: the Zealot's second blade 0.28 Normal s (5 ticks) after the first. Combat
/// resolution lands the first strike on the release tick and keeps the rest here, one every
/// [`WeaponSpec::strike_interval_ticks`]. A weapon whose first strike is delayed
/// ([`WeaponSpec::first_strike_delay_ticks`], the Liberator's first period; plan
/// `2026-11-23-strike-delays`) keeps the whole burst here from the release, the first strike
/// included. A pending strike lands only if its target is still a valid target on the weapon's
/// plane and, where the weapon names one, within its [`WeaponSpec::followup_max_distance`]; a
/// strike that fails its condition is lost, and the strikes after it still try. The attacker's
/// death or boarding a transport ends the burst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingStrikes {
    /// Entity the attack was released at; every strike of the burst is aimed at it.
    pub target_id: EntityId,
    /// Combat plane of the weapon that released the attack; a strike lands only on a target
    /// still on that plane, and the weapon is resolved against it again for each strike.
    pub plane: MoverType,
    /// Strikes still to land, at least 1 while the burst is pending; the whole burst while a
    /// delayed first strike waits.
    pub remaining: u8,
    /// Ticks until the next strike lands, at least 1 between ticks.
    pub ticks_until_next: u16,
    /// Whether the weapon's burst is channeled ([`sim_types::WeaponSpec::channeled`]): an
    /// explicit order that replaces the attacker's orders ends it (Q0058).
    pub channeled: bool,
}

/// An active timed buff or debuff affecting unit stats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveBuff {
    /// Distinct buff identifier (1 = Stimpack, 3 = attack dash; slowing debuffs
    /// are statuses of the effect engine).
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
    /// Speed per tick, world units, at the start of the next tick: a constant missile's one
    /// speed, or a mover missile's current speed (plan `2026-11-27-weapon-delivery` D-2).
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
    /// The target's [`Entity::teleport_count`] when the projectile was launched or last locked
    /// onto it; a target whose count has moved on since teleported in flight (plan
    /// `2026-11-24-weapon-validation` D-2). Zero for a projectile without a unit target.
    pub target_teleport_count: u16,
    /// Index into `weapon_spec.missile_movers` of the mover chosen at the release by the edge
    /// distance to the target (`IfRangeLTE`, plan `2026-11-27-weapon-delivery` D-2); `None` for
    /// a missile flying the constant `speed`.
    pub mover: Option<u8>,
    /// Index of the chosen mover's phase the missile flies; zero for a constant missile.
    pub phase: u8,
    /// Path flown since the release, world units, which ends each phase (`Outro`); zero for a
    /// constant missile, whose flight reads no phase.
    pub travelled: SimScalar,
}

/// What a unit remembers about its movement order between ticks.
///
/// All of this is authoritative: it decides when an order ends, so it is hashed by the checksum.
/// A new order starts it afresh, apart from [`Self::abandoned_target`], which outlives orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent hashed flags of one unit's movement, each read by a different rule"
)]
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
    /// Sense in which this unit rounds a body parked on its next corner: `1` counter-clockwise
    /// round the body, `-1` clockwise, `0` none chosen. Chosen when the unit first aims past the
    /// body and kept until the corner is passed or the route replaced, so the unit does not swap
    /// sides while it rounds the body (plan `2026-11-09-sc2-pathfinding` D-9 A3). While it is
    /// set, the decision to round is held too: any non-yielding body touching the corner, moving
    /// or not, keeps the unit rounding it (plan `2026-11-19-parked-body-aim` D-1).
    pub corner_round: i8,
    /// Whether this unit is walking round a crowd that blocks its way to its goal, on the side
    /// `flank_left` keeps (the surround walk, plan `2026-11-09-friendly-blocking` D-2).
    pub flank_active: bool,
    /// Whether the surround walk has already turned back once, its first side having closed.
    pub flank_reversed: bool,
    /// Whether the surround walk has ended without a way in (both sides closed, or a full circle
    /// walked); the unit then waits until its way forward opens, the side that closed last opens
    /// for a whole step (a walk spent by closed sides only), it gains ground, or its goal changes
    /// (plan `2026-11-19-crowd-edge-steering-2` D-2).
    pub flank_spent: bool,
    /// Consecutive ticks the surround walk's side has been closed (`FLANK_REVERSE_TICKS`).
    pub flank_blocked_ticks: u16,
    /// Distance walked round the crowd on this walk, in world units.
    pub flank_walked: SimScalar,
    /// Distance the surround walk has stepped back from its goal since it last became active, in
    /// world units: the sum of each walk step's backward part along the way in. A walk in
    /// progress ends only when the straight way in is free for a step plus this distance, so it
    /// does not end on the room its own retreat made (plan `2026-11-19-crowd-edge-steering-2`
    /// D-1). Cleared when the unit steps forward, when the walk ends and on a new order.
    pub flank_retreat: SimScalar,
    /// The target the surround walk approaches; `None` for an attack-move without one. A walk
    /// for another goal starts afresh.
    pub flank_target: Option<EntityId>,
    /// Where this unit stood when its current automatic chase began, in world units: the leash of
    /// that chase is measured from here (5.0.14's `AcquireLeashRadius`, plan
    /// `2026-11-18-contour-footprints` Amendment 4, B-142). `None` while it chases no automatically
    /// acquired target; an ordered attack and a new order clear it.
    pub leash_origin: Option<SimVec2>,
    /// The body this unit passes on its committed side: the partner of the side rule's one-pair
    /// commitment (plan `2026-11-20-contact-solver` I-3). Written only by the solver pipeline;
    /// `None` under the legacy pipeline and when no pair is committed.
    pub side_partner: Option<EntityId>,
    /// The committed side of the side rule: the unit passes [`Self::side_partner`] on its left
    /// when true (I-3). Written only by the solver pipeline.
    pub side_left: bool,
    /// The gap this attacker walked to last tick: the free point of the ring round its target
    /// (its weapon band's outer edge) nearest its bearing, kept while it stays free (plan
    /// `2026-11-20-contact-solver` Amendment 7, item 2). One remembered point, not a claim:
    /// nobody else reads it. Written only by the solver pipeline; `None` under the legacy
    /// pipeline and while the unit's own bearing point is free.
    pub slot: Option<SlotClaim>,
    /// Where this unit's player last saw the target of the unit's attack order, or of its follow
    /// of another player's unit, and whether the player has lost it since (plan
    /// `2026-11-24-weapon-validation` Amendment 2, B-195; Q0106 part 1 (a); plan
    /// `2026-11-26-follow-order` D-5). Written by target acquisition, for a player's attack order
    /// or such a follow only, from the first tick the player sees the target under the order;
    /// read by movement, which walks a unit whose target is lost to the point instead of chasing
    /// or following it. `None` while the order's target has not been seen under it, and under
    /// any other order.
    pub last_seen: Option<LastSeen>,
    /// Where the target of this unit's follow stood, at the start of the tick, when the unit came
    /// to rest on it, world units (plan `2026-11-26-follow-order` D-4). The follower walks again
    /// once its target's tick-start position lies more than the movement rules'
    /// `follow_range_slop` from here. `None` while it walks, and under any other order.
    pub follow_rest: Option<SimVec2>,
}

/// The last sighting of an attack order's target by the attacker's player (plan
/// `2026-11-24-weapon-validation` Amendment 2, B-195).
///
/// "Seen" is the weapon filter's rule ([`crate::SimWorld`]'s `sees_enemy`): the cell at the
/// target's centre is `Visible` to the player, or the target is attack-revealed to it (Q0106 part
/// 4 (a)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastSeen {
    /// The target of the attack order this sighting belongs to.
    pub target: EntityId,
    /// The target's position when it was last seen, world units; an unreachable point is replaced
    /// by the nearest reachable one the walk found.
    pub point: SimVec2,
    /// Whether the player has stopped seeing the target since: the unit then walks to
    /// [`Self::point`] and its order ends there.
    pub lost: bool,
}

/// The gap an attacker walks to on the ring round its target (plan `2026-11-20-contact-solver`
/// Amendment 7, item 2): renewed every tick from the tick-start snapshot, kept while it stays
/// free and reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotClaim {
    /// The target the slot is round.
    pub target: EntityId,
    /// The gap's point, world units: on the ring (the band's outer edge) round the target's
    /// tick-start position of the tick it was chosen.
    pub point: SimVec2,
}

impl MovementMemory {
    /// Clears everything tied to the current order, the side rule's commitment and any slot
    /// claim included.
    pub(crate) fn restart(&mut self) {
        *self = Self {
            abandoned_target: self.abandoned_target,
            abandoned_ticks: self.abandoned_ticks,
            flank_left: self.flank_left,
            ..Self::default()
        };
    }

    /// Records what the unit's player sees of `target`, the target of its attack order, this
    /// tick: `Some(position)` (world units) when it sees the target there, `None` when it does
    /// not. A target not seen under the order before has nothing to lose, so `None` then changes
    /// nothing. When the target is lost or found again, the goal of the unit's walk changes
    /// between the target and the point, so the progress measure and the solver's ring gap start
    /// afresh, as for a new order (plan `2026-11-24-weapon-validation` Amendment 2, B-195).
    pub(crate) fn sight(&mut self, target: EntityId, seen_at: Option<SimVec2>) {
        let known = self.last_seen.filter(|seen| seen.target == target);
        let next = match (seen_at, known) {
            (Some(point), _) => LastSeen {
                target,
                point,
                lost: false,
            },
            (None, Some(seen)) => LastSeen { lost: true, ..seen },
            (None, None) => return,
        };
        if known.is_some_and(|seen| seen.lost != next.lost) {
            self.progress_best = None;
            self.stalled_ticks = 0;
            self.slot = None;
        }
        self.last_seen = Some(next);
    }

    /// The point this unit walks to instead of chasing `target`: [`LastSeen::point`] when the
    /// last sighting belongs to `target` and the target is lost (Amendment 2).
    #[must_use]
    pub(crate) fn lost_target_point(&self, target: EntityId) -> Option<SimVec2> {
        self.last_seen
            .filter(|seen| seen.target == target && seen.lost)
            .map(|seen| seen.point)
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
    /// Ticks left waiting at the node with cargo before the return leg (the resource's
    /// `ReturnDelay`, plan `2026-11-22-harvest-timing` D-1); zero when not waiting.
    pub return_ticks: u16,
    /// Ticks spent waiting for a site or for ground to clear, or for a resource to be free
    /// (plan `2026-11-23-harvest-occupancy` D-2: the line's order, most ticks first).
    pub wait_ticks: u16,
    /// Depot a carrying unit is heading for.
    pub depot: Option<EntityId>,
    /// Node last gathered from, where `ReturnCargo` resumes. Outlives orders.
    pub last_node: Option<ResourceNodeId>,
    /// The structure this gatherer is inside, hidden, while it gathers (plan
    /// `2026-11-24-hidden-harvesters` D-2, D-3); `None` otherwise.
    ///
    /// **Invariant:** `Some(s)` implies that the front order is `Harvest`, that `gather_ticks`
    /// is above 0, and that `s` is alive and its archetype hides its harvesters. The line sets it
    /// when it grants the node; the take, every new leg ([`crate::economy`]'s `fresh_leg`) and
    /// the structure's death clear it.
    pub housed_in: Option<EntityId>,
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
    /// Searches of the last spaced line area still to land; `None` when none are pending.
    pub pending_line: Option<PendingLine>,
    /// Active temporary stat buffs and debuffs.
    pub active_buffs: Vec<ActiveBuff>,
    /// The damage-absorbing barrier's window, remaining absorption and cooldown (B-41).
    pub barrier: BarrierState,
    /// Ticks until the attack dash may start again ([`AttackDashSpec::cooldown_ticks`], B-43);
    /// 0 when it is ready and for a unit without the capability.
    pub attack_dash_cooldown: u16,
    /// FIFO production jobs: archetype, remaining 25 Hz updates and reserved supply.
    /// Only the front job advances per update, preserving the authored `u32` duration.
    pub production_queue: VecDeque<crate::production::ProductionJob>,
    /// Supply consumption captured at spawn; released only when this entity is reaped.
    pub captured_supply_used: u32,
    /// Completed supply provision captured at spawn, independent of later catalog edits.
    pub captured_supply_provided: u32,
    /// Where this production structure sends the units it makes: one route per rally slot that
    /// holds a rally (plans `2026-11-23-production-queue` D-3 and `2026-11-25-rallies` D-2),
    /// empty without a rally. Sorted by slot, at most one route per slot, no route empty, every
    /// slot below the archetype's slot count and every route within its waypoint cap. Set by
    /// `SetRally` and by the town hall's auto-rally at creation (D-7).
    pub rallies: Vec<sim_types::RallyRoute>,
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
    /// The enemy this unit's weapon works against inside an attack order whose target it cannot
    /// work against this tick, for a weapon whose data lets it fire at other targets
    /// ([`WeaponSpec::fires_at_other_targets`], plan `2026-11-24-fire-rules` D-1, D-4).
    /// Written only by target acquisition and read only by the combat phase; pursuit never reads
    /// it, so the unit keeps chasing its order's target. Cleared by every order that replaces
    /// the unit's orders, by boarding a transport, and when acquisition releases it.
    pub weapon_target: Option<EntityId>,
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
    // PERFORMANCE RATIONALE:
    // Why: An inline spec made `Entity`'s size follow `WeaponSpec`'s; a 24-byte weapon field moved
    // the code LLVM emits for the `BTreeMap<EntityId, Entity>` lookup in target acquisition by
    // 2.19 % of the throughput bench's instructions (perf record, `scan_candidates`).
    // What: Box the spec, so `Entity`'s size and layout no longer depend on the weapon's.
    // Trade-off: One allocation when a unit enters a mode, and one indirection on reading it.
    pub siege_weapon: Option<Box<WeaponSpec>>,
    /// Active generalized stance / mode identifier (e.g. "`siege_mode`", "burrowed").
    pub active_mode: Option<String>,
    /// Remaining ticks of channeled casting / transformation.
    pub channeling_remaining_ticks: u16,
    /// Pending ability queued or being channeled: (`ability_index`, `target_pos`, `target_entity`).
    pub pending_ability: Option<(u8, Option<SimVec2>, Option<EntityId>)>,
    /// Independent ability cooldown timers for up to 4 ability slots.
    pub ability_cooldowns: [u16; 4],
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
    /// How many `Move` and `AttackMove` orders have ever entered [`Self::order_queue`], wrapping
    /// (plan `2026-11-10-group-routing`, D-1). An order's ordinal is this count before it
    /// entered, so the active route order's ordinal is this count minus the route orders still
    /// queued ([`Self::active_route_ordinal`]); route groups bind members by it. Only
    /// [`Self::enqueue_order`] changes it.
    pub route_orders_issued: u32,
    /// Ticks the unit stays locked by an uninterruptible stage: a cast's backswing
    /// (`CastOutroTime`, plan `2026-11-10-spellcaster-mechanics` D-7.4) or the rest of a
    /// harvest's `WaitToReturn` that a new order cut into (plan `2026-11-23-harvest-occupancy`
    /// D-5). No movement, attack or other cast meanwhile; its orders wait.
    pub cast_lock_ticks: u16,
    /// The flight a `Displace` effect node gives the unit (D-8); `None` when not displaced.
    pub displacement: Option<crate::effects::Displacement>,
    /// For each player, the ticks left during which this unit is revealed to that player by its
    /// own attacks: a strike of a weapon whose attack visibility is `Visible` on a unit of player
    /// p raises the count for p to the weapon's reveal ticks, and the vision phase counts it down
    /// (plan `2026-11-23-vision-and-detection` D-5). All zero at spawn.
    pub attack_reveal_ticks: PerPlayer<u16>,
    /// How many times the unit has been relocated by a teleport, wrapping: written only by
    /// [`Self::teleport_to`], compared by missiles in flight, never reset (plan
    /// `2026-11-24-weapon-validation` D-2). Zero at spawn.
    pub teleport_count: u16,
    /// The combined modifiers of the statuses this unit carries: a cache of the fold of
    /// `SimWorld::effects` for this entity, refreshed by the effect engine whenever one of its
    /// statuses is inserted or removed, and so derived state, left out of the checksum.
    pub(crate) status_modifiers: crate::effects::StatusModifiers,
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
    /// The unit's hangar, exactly on entities whose archetype has one (plan
    /// `2026-11-28-carrier-interceptors` D-3); hashed.
    pub(crate) hangar: Option<crate::hangar::HangarState>,
    /// On a launched hangar member, its hangar unit and whether it is returning (D-3); hashed.
    pub(crate) hangar_link: Option<crate::hangar::HangarLink>,
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

    /// Whether this unit has nothing to do: no order, or `Idle` or `Stop` at the front. A unit
    /// standing still under any other order is waiting, not idle, and friends do not push it
    /// aside (plan `2026-11-09-friendly-blocking` D-4).
    #[must_use]
    pub fn is_idle(&self) -> bool {
        matches!(
            self.order_queue.front(),
            None | Some(UnitOrder::Idle | UnitOrder::Stop)
        )
    }

    /// Whether this unit stands as an idle body for contact: it [is idle](Self::is_idle), or it
    /// rests under a follow ([`MovementMemory::follow_rest`] set; plan `2026-11-26-follow-order`
    /// D-7). Moving friends push such a body aside, as they push an idle one. Acquisition, the
    /// HUD and the tasks read [`Self::is_idle`], whose meaning does not change.
    #[must_use]
    pub fn stands_idle(&self) -> bool {
        self.is_idle()
            || (matches!(self.order_queue.front(), Some(UnitOrder::Follow { .. }))
                && self.movement.follow_rest.is_some())
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
            attack_dash_cooldown: 0,
            pending_strikes: None,
            pending_line: None,
            active_buffs: Vec::new(),
            production_queue: VecDeque::new(),
            captured_supply_used: archetype.supply_cost,
            captured_supply_provided: archetype.supply_provided,
            rallies: Vec::new(),
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
            weapon_target: None,
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
            is_cloaked: false,
            max_hp: archetype.max_hp,
            is_completed: true,
            transport_carrier: None,
            is_invulnerable: false,
            route_orders_issued: 0,
            cast_lock_ticks: 0,
            displacement: None,
            attack_reveal_ticks: PerPlayer::default(),
            teleport_count: 0,
            status_modifiers: crate::effects::StatusModifiers::NEUTRAL,
            hull_turn_budget: archetype.turn_rate.raw(),
            // The spawn stocks the free start members (`SimWorld::prepare_spawn`), which needs
            // the member's archetype (plan `2026-11-28-carrier-interceptors` D-3).
            hangar: archetype
                .capabilities
                .hangar
                .map(|spec| crate::hangar::HangarState::with_capacity(spec.max_count)),
            hangar_link: None,
        }
    }

    /// Returns the carrier identity while this actor is absent from world-space participation.
    /// The owning world validates membership, owner and lifecycle consistency.
    #[must_use]
    pub const fn transport_carrier(&self) -> Option<EntityId> {
        self.transport_carrier
    }

    /// The combined modifiers of the statuses this unit carries (plan
    /// `2026-11-10-spellcaster-mechanics` D-3); neutral without statuses.
    #[must_use]
    pub const fn status_modifiers(&self) -> crate::effects::StatusModifiers {
        self.status_modifiers
    }

    /// Whether the unit may not act of its own accord this tick: stunned by a status, locked
    /// after a cast (`cast_lock_ticks`, D-7.4) or flown by a displacement (D-8). Such a unit
    /// does not walk, attack, acquire or cast; its orders wait.
    #[must_use]
    pub const fn is_action_locked(&self) -> bool {
        self.status_modifiers.stun || self.cast_lock_ticks > 0 || self.displacement.is_some()
    }

    /// Relocates the unit to `destination` (world units) by a teleport: sets the position,
    /// stops it, clears its funnel waypoints and counts the teleport in
    /// [`Self::teleport_count`] (wrapping), so missiles in flight at it can tell (plan
    /// `2026-11-24-weapon-validation` D-2).
    ///
    /// Invariant: every instantaneous relocation that 5.0.14 performs with a `CEffectTeleport`
    /// goes through this method. Displacements that fly (`Displace`), unloads and placement do
    /// not.
    pub fn teleport_to(&mut self, destination: SimVec2) {
        self.position = destination;
        self.velocity = SimVec2::ZERO;
        self.current_funnel_waypoints.clear();
        self.teleport_count = self.teleport_count.wrapping_add(1);
    }

    /// Whether this unit's attacks reveal it to `player` now: a counter of
    /// [`Self::attack_reveal_ticks`] above zero (plan `2026-11-23-vision-and-detection` D-5).
    /// False for a player outside the slots.
    #[must_use]
    pub fn is_attack_revealed_to(&self, player: PlayerId) -> bool {
        self.attack_reveal_ticks
            .get(player)
            .is_some_and(|&ticks| ticks > 0)
    }

    /// Whether the unit is hidden by cloak or burrow now: cloaked and not revealed by a status
    /// (plan D-11). A revealed unit is seen like any other unit wherever its owner's enemies
    /// have vision.
    #[must_use]
    pub const fn is_concealed(&self) -> bool {
        self.is_cloaked && !self.status_modifiers.reveal
    }

    /// Whether the unit takes part in the movement and collision phases: alive, in the world
    /// and not flown by a displacement, whose position the effect engine drives (D-8).
    #[must_use]
    pub fn takes_part_in_movement(&self) -> bool {
        self.is_alive() && !self.is_loaded_in_transport() && self.displacement.is_none()
    }

    /// The unit's hangar, if its archetype has one (plan `2026-11-28-carrier-interceptors` D-3).
    #[must_use]
    pub const fn hangar(&self) -> Option<&crate::hangar::HangarState> {
        self.hangar.as_ref()
    }

    /// The link of a launched hangar member to its hangar unit (D-3); `None` for every other
    /// unit.
    #[must_use]
    pub const fn hangar_link(&self) -> Option<crate::hangar::HangarLink> {
        self.hangar_link
    }

    /// Whether this actor is carried; derived solely from its authoritative carrier link.
    #[must_use]
    pub const fn is_loaded_in_transport(&self) -> bool {
        self.transport_carrier.is_some()
    }

    /// The structure this unit is inside while it gathers, if any (plan
    /// `2026-11-24-hidden-harvesters` D-3); see [`UnitTask::housed_in`].
    #[must_use]
    pub const fn housing(&self) -> Option<EntityId> {
        self.task.housed_in
    }

    /// Whether this unit is inside a structure that hides its harvesters, gathering (plan
    /// `2026-11-24-hidden-harvesters` D-2).
    #[must_use]
    pub const fn is_housed(&self) -> bool {
        self.task.housed_in.is_some()
    }

    /// Whether this unit is in the editor's filter state `Hidden` (plan
    /// `2026-11-24-hidden-harvesters` D-4): carried in a transport, or inside a structure that
    /// hides its harvesters. In `StarCraft` II 5.0.14 weapons (144 of 146 `TargetFilters`),
    /// acquisition (`AcquireFilters`), effect searches (199 of 247 `SearchFilters`), loading and
    /// the `CasterIsNotHidden` validator exclude `Hidden` units, so a hidden unit is not
    /// acquired, attacked, splashed, found by a search or loaded, and it casts, acquires and
    /// sees nothing.
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.is_loaded_in_transport() || self.is_housed()
    }

    /// Whether other units pass through this one in the movement phase: it is no collision pair
    /// and no obstacle anyone steers or is pushed around, in either contact pipeline. True for a
    /// [hidden](Self::is_hidden) unit (plan `2026-11-24-hidden-harvesters` D-11, Amendment 1), a
    /// harvester on its mining walk (`is_mineral_walking`) and a unit whose statuses suppress
    /// collision. The unit itself still moves.
    #[must_use]
    pub const fn passes_through_units(&self) -> bool {
        self.is_hidden() || self.is_mineral_walking || self.status_modifiers.suppress_collision
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

    /// Whether the unit's front order walks it (plan `2026-11-23-turret-wind-up` D-3): a move or
    /// an attack-move, a task order with a goal to walk to, a cast, whose approach may walk, or a
    /// follow while its follower does not rest (plan `2026-11-26-follow-order` D-4).
    /// It is the classification the movement phase's `walking_goal` makes, read from the order
    /// and not from the displacement, so a collision push never counts as walking. An attack
    /// order's pursuit is not counted: inside an attack order no weapon is gated by movement.
    #[must_use]
    pub(crate) fn order_walks(&self) -> bool {
        match self.order_queue.front() {
            Some(
                UnitOrder::Move { .. }
                | UnitOrder::AttackMove { .. }
                | UnitOrder::CastAbility { .. },
            ) => true,
            Some(UnitOrder::Follow { .. }) => self.movement.follow_rest.is_none(),
            Some(_) => self.has_task_order() && self.task.goal.is_some(),
            None => false,
        }
    }

    /// Drops everything tied to the current task leg; what outlives an order stays.
    pub(crate) fn abandon_task_leg(&mut self) {
        self.task = crate::economy::fresh_leg(self.task);
        self.is_mineral_walking = false;
    }

    /// Replaces this unit's orders after a mode switch given without Shift: the queue empties
    /// and Hold Position ends, as a `Stop` replaces them, but the weapon keeps its phase.
    ///
    /// `StarCraft` II's morphs (Siege Mode, the Lurker's burrow) are not `Transient`, so a
    /// non-queued one replaces the unit's orders; a `Transient` ability such as Stimpack does
    /// not (`liberty` AbilData:1231; plan `2026-11-09-client-mode-commands` D-6a). The caller
    /// has already cleared velocity and funnel waypoints when the cast committed.
    pub(crate) fn replace_orders_after_mode_switch(&mut self) {
        self.movement.restart();
        self.abandon_task_leg();
        self.order_queue.clear();
        self.is_holding_position = false;
    }

    /// Handles player move input during combat, executing SC2 backswing cancellation: `order` is
    /// a `Move`, or a `Follow` of kind `Move`, the same command of the move ability on a unit.
    fn handle_interrupting_move(&mut self, order: UnitOrder) {
        match self.weapon_phase {
            WeaponPhase::Windup { .. } => {
                // ABORT: Shot was winding up; moving cancels the shot with zero damage.
                self.weapon_phase = WeaponPhase::Ready;
                self.order_queue.clear();
                self.enqueue_order(order);
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
            WeaponPhase::Backswing { .. } => {
                // CANCEL BACKSWING: Damage has already been applied!
                // Terminate the backswing animation immediately.
                // The weapon cooldown continues to tick down in the background.
                self.weapon_phase = WeaponPhase::Cooldown;
                self.order_queue.clear();
                self.enqueue_order(order);
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
            _ => {
                self.order_queue.clear();
                self.enqueue_order(order);
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

    /// Appends `order` to the back of [`Self::order_queue`], counting it in
    /// [`Self::route_orders_issued`] when it is a `Move` or `AttackMove`.
    ///
    /// Every insertion of a route order goes through here, so a route group's ordinal binding
    /// (plan `2026-11-10-group-routing`, D-1) stays exact. Other orders may be pushed through it
    /// too; they leave the counter alone.
    pub fn enqueue_order(&mut self, order: UnitOrder) {
        if is_route_order(&order) {
            self.route_orders_issued = self.route_orders_issued.wrapping_add(1);
        }
        self.order_queue.push_back(order);
    }

    /// The ordinal of the active route order: the first `Move` or `AttackMove` in the queue, or,
    /// with none queued, the ordinal the next one will get (plan `2026-11-10-group-routing`,
    /// D-1). Route orders leave the queue only from the front or by clearing, so this is exact.
    #[must_use]
    pub fn active_route_ordinal(&self) -> u32 {
        let queued = self
            .order_queue
            .iter()
            .filter(|o| is_route_order(o))
            .count();
        // A queue never holds 2^32 orders; saturation keeps the arithmetic total regardless.
        self.route_orders_issued
            .wrapping_sub(u32::try_from(queued).unwrap_or(u32::MAX))
    }

    /// Commits a follow of `target` after world selection preflight (plan
    /// `2026-11-26-follow-order` D-2): queued behind the orders when `queue`, otherwise in
    /// place of them, as a move or a scan move replaces them. A follow is no route order, so it
    /// leaves [`Self::route_orders_issued`] alone.
    pub(crate) fn commit_follow_order(&mut self, target: EntityId, kind: FollowKind, queue: bool) {
        self.commit_order(UnitOrder::Follow { target, kind }, queue);
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

    /// Carries the rest of a harvest's `ReturnDelay` wait into the action lock before a
    /// non-queued order clears the leg (plan `2026-11-23-harvest-occupancy` D-5, B-160): 5.0.14
    /// marks the stage uninterruptible (`UninterruptibleArray WaitToReturn 1`, core
    /// AbilData:379), so the new order starts on the tick the wait would have ended, as a cast's
    /// backswing holds the next order. The cargo stays on the unit.
    fn hold_through_return_wait(&mut self) {
        if matches!(self.order_queue.front(), Some(UnitOrder::Harvest { .. }))
            && self.task.return_ticks > 0
        {
            self.cast_lock_ticks = self.cast_lock_ticks.max(self.task.return_ticks);
        }
    }

    // An immediate UseStim is refused by the public wrapper; the concrete movement
    // helper only constructs Move/AttackMove. No fallible work follows admission.
    fn commit_order(&mut self, order: UnitOrder, queue: bool) {
        if queue {
            self.enqueue_order(order);
            return;
        }

        self.hold_through_return_wait();
        // The weapon's own target belongs to the attack order being replaced (plan
        // `2026-11-24-fire-rules` D-1); acquisition chooses anew under the new order.
        self.weapon_target = None;
        self.movement.restart();
        self.abandon_task_leg();
        self.end_channeled_burst_for(&order);
        // An explicit non-attack order cancels the attack, and with it a running dash; an
        // attack on another target or an attack-move keeps it (the owner's engine note on
        // Q0071).
        if matches!(
            order,
            UnitOrder::Move { .. }
                | UnitOrder::Follow {
                    kind: FollowKind::Move,
                    ..
                }
                | UnitOrder::Stop
                | UnitOrder::HoldPosition
        ) {
            self.end_attack_dash();
        }
        match order {
            UnitOrder::Move { .. }
            | UnitOrder::Follow {
                kind: FollowKind::Move,
                ..
            } => {
                self.handle_interrupting_move(order);
            }
            UnitOrder::HoldPosition => {
                self.is_holding_position = true;
                self.velocity = SimVec2::ZERO;
                self.order_queue.clear();
                self.enqueue_order(UnitOrder::HoldPosition);
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
                self.enqueue_order(UnitOrder::AttackTarget { target, origin });
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
            _ => {
                if matches!(self.weapon_phase, WeaponPhase::Windup { .. }) {
                    self.weapon_phase = WeaponPhase::Ready;
                }
                self.order_queue.clear();
                self.enqueue_order(order);
                self.current_funnel_waypoints.clear();
                self.is_holding_position = false;
            }
        }
    }

    /// Ends a pending channeled burst that `order`, replacing this unit's orders, interrupts.
    ///
    /// In `StarCraft` II a multi-strike weapon whose burst effect sets `Channeled` (the
    /// Zealot's `PsiBladesBurst`, `liberty` EffectData:1924) channels it under the attack order,
    /// so any explicit order that replaces that order between the strikes cancels the strikes
    /// still to land (owner answer to Q0058). Only an attack on the burst's own target keeps it.
    /// A burst that is not channeled (the Viking's `LanzerTorpedoes`) always runs to its end.
    fn end_channeled_burst_for(&mut self, order: &UnitOrder) {
        let Some(pending) = &self.pending_strikes else {
            return;
        };
        let keeps =
            matches!(order, UnitOrder::AttackTarget { target, .. } if *target == pending.target_id);
        if pending.channeled && !keeps {
            self.pending_strikes = None;
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
        if let Some(buff) = self
            .active_buffs
            .iter_mut()
            .find(|b| b.buff_id == OVERCLOCK_BUFF_ID)
        {
            buff.remaining_ticks = spec.duration_ticks;
            buff.speed_multiplier = spec.speed_multiplier;
            buff.attack_rate_multiplier = spec.attack_rate_multiplier;
        } else {
            self.active_buffs.push(ActiveBuff {
                buff_id: OVERCLOCK_BUFF_ID,
                remaining_ticks: spec.duration_ticks,
                speed_multiplier: spec.speed_multiplier,
                attack_rate_multiplier: spec.attack_rate_multiplier,
            });
        }
    }

    /// Starts an attack dash: the speed buff [`ATTACK_DASH_BUFF_ID`] for `spec.duration_ticks`
    /// ticks, counting this one, and the dash's cooldown (B-43). The caller has checked that
    /// the dash is ready and its trigger window holds.
    ///
    /// # Errors
    /// Rejects malformed or unrepresentable buff state without changing anything.
    pub(crate) fn begin_attack_dash(
        &mut self,
        spec: &AttackDashSpec,
    ) -> Result<(), EntityAdmissionError> {
        self.prospective_speed_multiplier(ATTACK_DASH_BUFF_ID, spec.speed_multiplier)?;
        if let Some(buff) = self
            .active_buffs
            .iter_mut()
            .find(|b| b.buff_id == ATTACK_DASH_BUFF_ID)
        {
            buff.remaining_ticks = spec.duration_ticks;
            buff.speed_multiplier = spec.speed_multiplier;
        } else {
            self.active_buffs.push(ActiveBuff {
                buff_id: ATTACK_DASH_BUFF_ID,
                remaining_ticks: spec.duration_ticks,
                speed_multiplier: spec.speed_multiplier,
                attack_rate_multiplier: SimScalar::ONE,
            });
        }
        self.attack_dash_cooldown = spec.cooldown_ticks;
        Ok(())
    }

    /// Ends a running attack dash, as the unit's weapon starting a swing does in `StarCraft` II
    /// (`Charging`'s remove validator `ChargeTargetAttacked`, `liberty` BehaviorData:967-975,
    /// ValidatorData:3007-3011), and as an explicit move, stop or hold-position order does (the
    /// owner's engine note on Q0071). Leaves its cooldown running; does nothing without a dash.
    pub(crate) fn end_attack_dash(&mut self) {
        self.active_buffs
            .retain(|buff| buff.buff_id != ATTACK_DASH_BUFF_ID);
    }

    /// Whether an attack dash is running.
    #[must_use]
    pub fn is_attack_dashing(&self) -> bool {
        self.active_buffs
            .iter()
            .any(|buff| buff.buff_id == ATTACK_DASH_BUFF_ID)
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
                    self.siege_weapon = spec.ground_weapon_override.clone().flatten().map(Box::new);
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
                    self.siege_weapon = Some(Box::new(WeaponSpec::clone(sieged_weapon)));
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

    /// Whether this unit is burrowed: its active mode is a [`UnitModeSpec::burrowed`] one, or a
    /// burrowing cloak toggle (`CloakToggle { is_burrow: true }`) has put it under ground. A
    /// burrowed body touches only other burrowed bodies (plan `2026-11-09-friendly-blocking` R6,
    /// B-93). A unit still channelling its burrow is not yet burrowed, as its other mode stats
    /// are not. `archetype` must be this unit's.
    #[must_use]
    pub fn is_burrowed(&self, archetype: &UnitArchetype) -> bool {
        let burrow_toggle = || {
            archetype.abilities.iter().any(|ability| {
                matches!(
                    ability.effect,
                    AbilityEffectSpec::CloakToggle {
                        is_burrow: true,
                        ..
                    }
                )
            })
        };
        self.active_mode
            .as_deref()
            .and_then(|mode| archetype.get_mode(mode))
            .is_some_and(|mode| mode.burrowed)
            || (self.is_cloaked && self.is_immobile && burrow_toggle())
    }

    /// The mode this unit is in or, while it channels a switch, the mode it will be in when the
    /// channel ends; `None` for the archetype's default stance. `archetype` must be this unit's.
    ///
    /// Reads `active_mode`, `channeling_remaining_ticks` and `pending_ability` exactly as the end
    /// of a channel resolves them (`upkeep_entity`, then [`Self::apply_mode_transition`]): a
    /// pending `SwitchMode` of the active mode settles in no mode, a pending `SwitchMode` of any
    /// other declared mode settles in that mode, a pending index the archetype lacks settles in
    /// no mode, and any other pending effect leaves the stance as it is. Read-only, so the
    /// client's command card and its admission share the simulation's rule (plan
    /// `2026-11-09-client-mode-commands` D-3).
    #[must_use]
    pub fn settled_mode<'a>(&self, archetype: &'a UnitArchetype) -> Option<&'a UnitModeSpec> {
        let active = self.active_mode.as_deref();
        let settled = match self.pending_ability {
            Some((index, _, _)) if self.channeling_remaining_ticks > 0 => {
                match archetype.abilities.get(usize::from(index)) {
                    Some(ability) => match &ability.effect {
                        AbilityEffectSpec::SwitchMode { mode_name }
                            if archetype.get_mode(mode_name).is_some() =>
                        {
                            (active != Some(mode_name.as_str())).then_some(mode_name.as_str())
                        }
                        _ => active,
                    },
                    None => None,
                }
            }
            _ => active,
        };
        settled.and_then(|name| archetype.get_mode(name))
    }

    /// Whether this unit will be anchored once its current channel, if any, has ended.
    /// `archetype` must be this unit's.
    ///
    /// A unit that does not channel is anchored exactly when `is_immobile`; a channelling unit
    /// by its [`Self::settled_mode`], or by the archetype's default stance without one.
    #[must_use]
    pub fn immobile_after_channel(&self, archetype: &UnitArchetype) -> bool {
        if self.channeling_remaining_ticks == 0 {
            return self.is_immobile;
        }
        self.settled_mode(archetype).map_or_else(
            || archetype.is_immobile_by_default(),
            |mode| mode.is_immobile,
        )
    }

    /// The plain orders (Stop, Hold Position) this unit accepts in the stance it is in or is
    /// switching to ([`Self::settled_mode`]), by [`UnitArchetype::stance_orders`]. `archetype`
    /// must be this unit's. Read-only and allocation-free, so the client's command card and its
    /// admission share the simulation's rule (plan `2026-11-23-anchored-commands` D-2).
    #[must_use]
    pub fn settled_stance_orders(&self, archetype: &UnitArchetype) -> StanceOrders {
        archetype.stance_orders(self.settled_mode(archetype))
    }

    /// The A command of the stance this unit is in or is switching to ([`Self::settled_mode`]),
    /// by [`UnitArchetype::stance_orders`]: Attack, Scan Move or none (plan
    /// `2026-11-24-attack-orders` D-2). `archetype` must be this unit's. Read-only and
    /// allocation-free, so the client's card and its admission share the simulation's rule.
    #[must_use]
    pub fn settled_attack_command(&self, archetype: &UnitArchetype) -> AttackCommand {
        self.settled_stance_orders(archetype).attack
    }

    /// Whether an attack order on `target` reaches this unit (`StarCraft` II's `ToSelection`,
    /// `core` AbilData:127-129; plan `2026-11-24-attack-orders` D-2): the stance it is in or is
    /// switching to has [`AttackCommand::Attack`], `target` is another unit, and that stance's
    /// weapon against `target`'s plane ([`UnitArchetype::stance_weapon_against`]) exists and its
    /// `target_flags` meet `target`'s attributes, as automatic acquisition requires.
    ///
    /// The target's owner is not read: 5.0.14's attack ability leaves out own and allied units
    /// only for acquisition and the smart click (`core` AbilData:117-119, B-175), so an A-click on
    /// one's own unit attacks it. `archetype` must be this unit's and `target_archetype`
    /// `target`'s. Read-only and allocation-free; the simulation's admission
    /// (`SimWorld::issue_plain_order`) and the client's share it.
    #[must_use]
    pub fn can_attack(
        &self,
        archetype: &UnitArchetype,
        target: &Self,
        target_archetype: &UnitArchetype,
    ) -> bool {
        let stance = self.settled_mode(archetype);
        target.id != self.id
            && archetype.stance_orders(stance).attack == AttackCommand::Attack
            && archetype
                .stance_weapon_against(stance, target_archetype.mover_type)
                .is_some_and(|weapon| weapon.target_flags.intersects(target_archetype.attributes))
    }

    /// Whether this unit is armed in the stance it is in or is switching to
    /// ([`Self::settled_mode`]), by [`UnitArchetype::stance_armed`]. `archetype` must be this
    /// unit's. Read-only and allocation-free, so the client's Attack entry and its admission
    /// share the simulation's rule (plan `2026-11-23-stance-checked-orders` D-3).
    #[must_use]
    pub fn settled_armed(&self, archetype: &UnitArchetype) -> bool {
        archetype.stance_armed(self.settled_mode(archetype))
    }

    /// The sieged weapon replacing the archetype's ground weapon, if one is active.
    #[must_use]
    #[inline]
    pub fn active_siege_weapon(&self) -> Option<&WeaponSpec> {
        self.is_sieged
            .then_some(self.siege_weapon.as_deref())
            .flatten()
    }

    /// Whether the front order keeps this unit anchored in place.
    #[must_use]
    #[inline]
    pub fn is_on_hold(&self) -> bool {
        self.is_holding_position
            || matches!(self.order_queue.front(), Some(UnitOrder::HoldPosition))
    }

    /// Whether this unit's hull belongs to its weapon between shots: it holds position
    /// ([`Self::is_on_hold`]) or is immobile, so no pursuit aims it. The movement phase settles
    /// such a unit before it classifies the order (`walking_goal`), so `Pursuit::aim_if_in_range`,
    /// which aims every other unit under an attack order in its weapon's band, never reaches it.
    /// Between shots a hull weapon working for an attack order then turns the hull towards its
    /// target itself (plan `2026-11-25-wind-up-and-hold-facing` D-4, B-199; 5.0.14
    /// `FaceTargetWhileInCooldown`).
    #[must_use]
    #[inline]
    pub fn hull_aimed_by_weapon(&self) -> bool {
        self.is_on_hold() || self.is_immobile
    }

    /// Clears the order queue, hold, current target, route and weapon swing, as an effect's
    /// `CancelOrders` node does (plan `2026-11-10-spellcaster-mechanics` D-8): a swing in
    /// wind-up returns to ready, a backswing goes to cooldown. A mode channel in progress and a
    /// settled mode are kept.
    pub(crate) fn cancel_orders_for_effect(&mut self) {
        match self.weapon_phase {
            WeaponPhase::Windup { .. } => self.weapon_phase = WeaponPhase::Ready,
            WeaponPhase::Backswing { .. } => self.weapon_phase = WeaponPhase::Cooldown,
            _ => {}
        }
        self.end_attack_dash();
        self.order_queue.clear();
        self.current_funnel_waypoints.clear();
        self.is_holding_position = false;
        self.current_target = None;
        self.velocity = SimVec2::ZERO;
        self.movement.restart();
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

/// Whether `order` is a route order: a `Move` or `AttackMove`, the orders route groups bind to.
#[must_use]
pub const fn is_route_order(order: &UnitOrder) -> bool {
    matches!(order, UnitOrder::Move { .. } | UnitOrder::AttackMove { .. })
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

#[cfg(test)]
mod order_walks_tests {
    //! Plan `2026-11-23-turret-wind-up` D-3 (P-2): `Entity::order_walks` classifies every
    //! `UnitOrder` as the movement phase's `walking_goal` does.
    use crate::test_support::world;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        AttackOrigin, EntityId, FollowKind, MoverType, PlayerId, RallyTarget, ResourceNodeId,
        SimScalar, SimVec2, UnitOrder, UnitTypeId,
    };

    /// Whether `walking_goal` walks a unit whose front order is `order` and whose task leg has a
    /// goal when `task_goal`: a move or attack-move walks to its point, a task order to its leg's
    /// goal, a cast approaches its target, and a follow walks while it does not rest (its memory is
    /// fresh here). Every other order stands, or pursues as an attack (`AttackTarget`). The match lists every variant, so a new order fails to compile here
    /// until it is classified.
    fn walks(order: &UnitOrder, task_goal: bool) -> bool {
        match order {
            UnitOrder::Move { .. }
            | UnitOrder::AttackMove { .. }
            | UnitOrder::CastAbility { .. }
            | UnitOrder::Follow { .. } => true,
            UnitOrder::Harvest { .. }
            | UnitOrder::ReturnCargo
            | UnitOrder::Build { .. }
            | UnitOrder::Construct { .. }
            | UnitOrder::Load { .. }
            | UnitOrder::UnloadAll => task_goal,
            UnitOrder::Idle
            | UnitOrder::AttackTarget { .. }
            | UnitOrder::HoldPosition
            | UnitOrder::Stop
            | UnitOrder::UseStim
            | UnitOrder::Produce { .. }
            | UnitOrder::CancelConstruction
            | UnitOrder::Research { .. }
            | UnitOrder::CancelProduction { .. }
            | UnitOrder::SetRally { .. } => false,
        }
    }

    #[test]
    fn every_order_walks_as_the_movement_phase_classifies_it(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, 3);
        let kind = world
            .catalog
            .archetypes()
            .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .map(|arch| arch.unit_type_id)
            .ok_or("the shipped catalog has a mobile ground unit")?;
        let unit = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(8, 8))?;
        let point = SimVec2::from_i32(4, 4);
        let orders = [
            UnitOrder::Idle,
            UnitOrder::Move { target: point },
            UnitOrder::AttackMove { target: point },
            UnitOrder::AttackTarget {
                target: EntityId(99),
                origin: AttackOrigin::Explicit,
            },
            UnitOrder::HoldPosition,
            UnitOrder::Stop,
            UnitOrder::UseStim,
            UnitOrder::Produce {
                unit_type: UnitTypeId(0),
            },
            UnitOrder::Harvest {
                target: ResourceNodeId(0),
            },
            UnitOrder::ReturnCargo,
            UnitOrder::Build {
                structure_type: UnitTypeId(0),
                cell_x: 2,
                cell_y: 2,
            },
            UnitOrder::Construct {
                foundation: EntityId(99),
            },
            UnitOrder::CastAbility {
                ability_index: 0,
                target_pos: Some(point),
                target_entity: None,
            },
            UnitOrder::CancelConstruction,
            UnitOrder::Research { upgrade: 0 },
            UnitOrder::CancelProduction { job_index: 0 },
            UnitOrder::Load {
                carrier: EntityId(99),
            },
            UnitOrder::UnloadAll,
            UnitOrder::SetRally {
                target: RallyTarget::Point(point),
                slot: None,
            },
            UnitOrder::Follow {
                target: EntityId(99),
                kind: FollowKind::Move,
            },
        ];
        let entity = world.entities.get_mut(&unit).ok_or("unit")?;
        for order in orders {
            for task_goal in [false, true] {
                entity.order_queue = [order.clone()].into_iter().collect();
                entity.task.goal = task_goal.then_some(point);
                assert_eq!(
                    entity.order_walks(),
                    walks(&order, task_goal),
                    "{order:?}, task goal {task_goal}"
                );
            }
        }
        entity.order_queue.clear();
        assert!(!entity.order_walks(), "no order does not walk");
        Ok(())
    }

    /// Plan `2026-11-25-wind-up-and-hold-facing` D-4: the hull belongs to the weapon exactly when
    /// the movement phase settles the unit before pursuit: it holds (a front `HoldPosition`, or
    /// the hold flag under any front order) or it is immobile. Every other order shape, attack
    /// orders included, leaves the hull to the movement phase.
    #[test]
    fn the_hull_is_the_weapons_exactly_for_a_holding_or_immobile_unit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, 3);
        let kind = world
            .catalog
            .archetypes()
            .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .map(|arch| arch.unit_type_id)
            .ok_or("the shipped catalog has a mobile ground unit")?;
        let unit = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(8, 8))?;
        let point = SimVec2::from_i32(4, 4);
        let entity = world.entities.get_mut(&unit).ok_or("unit")?;
        let orders = [
            None,
            Some(UnitOrder::Idle),
            Some(UnitOrder::Stop),
            Some(UnitOrder::Move { target: point }),
            Some(UnitOrder::AttackMove { target: point }),
            Some(UnitOrder::AttackTarget {
                target: EntityId(99),
                origin: AttackOrigin::Explicit,
            }),
            Some(UnitOrder::AttackTarget {
                target: EntityId(99),
                origin: AttackOrigin::Acquired,
            }),
            Some(UnitOrder::HoldPosition),
        ];
        for order in orders {
            for holding_flag in [false, true] {
                for immobile in [false, true] {
                    entity.order_queue = order.clone().into_iter().collect();
                    entity.is_holding_position = holding_flag;
                    entity.is_immobile = immobile;
                    let holds = holding_flag || matches!(order, Some(UnitOrder::HoldPosition));
                    assert_eq!(
                        entity.hull_aimed_by_weapon(),
                        holds || immobile,
                        "{order:?}, hold flag {holding_flag}, immobile {immobile}"
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod weapon_target_tests {
    //! Plan `2026-11-24-fire-rules` D-1: the target a weapon chose inside an attack order belongs
    //! to that order, so every order that replaces the unit's orders clears it, and a queued
    //! order, which leaves the front order in place, keeps it.
    use crate::test_support::world;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        AttackOrigin, EntityId, FollowKind, MoverType, PlayerId, RallyTarget, ResourceNodeId,
        SimScalar, SimVec2, UnitOrder, UnitTypeId,
    };

    /// One order of every `UnitOrder` variant; the match keeps the list whole.
    fn every_order(point: SimVec2) -> Vec<UnitOrder> {
        let orders = vec![
            UnitOrder::Idle,
            UnitOrder::Move { target: point },
            UnitOrder::AttackMove { target: point },
            UnitOrder::AttackTarget {
                target: EntityId(99),
                origin: AttackOrigin::Explicit,
            },
            UnitOrder::HoldPosition,
            UnitOrder::Stop,
            UnitOrder::UseStim,
            UnitOrder::Produce {
                unit_type: UnitTypeId(0),
            },
            UnitOrder::Harvest {
                target: ResourceNodeId(0),
            },
            UnitOrder::ReturnCargo,
            UnitOrder::Build {
                structure_type: UnitTypeId(0),
                cell_x: 2,
                cell_y: 2,
            },
            UnitOrder::Construct {
                foundation: EntityId(99),
            },
            UnitOrder::CastAbility {
                ability_index: 0,
                target_pos: Some(point),
                target_entity: None,
            },
            UnitOrder::CancelConstruction,
            UnitOrder::Research { upgrade: 0 },
            UnitOrder::CancelProduction { job_index: 0 },
            UnitOrder::Load {
                carrier: EntityId(99),
            },
            UnitOrder::UnloadAll,
            UnitOrder::SetRally {
                target: RallyTarget::Point(point),
                slot: None,
            },
            UnitOrder::Follow {
                target: EntityId(99),
                kind: FollowKind::Move,
            },
        ];
        for order in &orders {
            match order {
                UnitOrder::Idle
                | UnitOrder::Move { .. }
                | UnitOrder::AttackMove { .. }
                | UnitOrder::AttackTarget { .. }
                | UnitOrder::HoldPosition
                | UnitOrder::Stop
                | UnitOrder::UseStim
                | UnitOrder::Produce { .. }
                | UnitOrder::Harvest { .. }
                | UnitOrder::ReturnCargo
                | UnitOrder::Build { .. }
                | UnitOrder::Construct { .. }
                | UnitOrder::CastAbility { .. }
                | UnitOrder::CancelConstruction
                | UnitOrder::Research { .. }
                | UnitOrder::CancelProduction { .. }
                | UnitOrder::Load { .. }
                | UnitOrder::UnloadAll
                | UnitOrder::SetRally { .. }
                | UnitOrder::Follow { .. } => {}
            }
        }
        orders
    }

    #[test]
    fn commit_order_clears_the_weapon_target_for_every_order(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, 3);
        let kind = world
            .catalog
            .archetypes()
            .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .map(|arch| arch.unit_type_id)
            .ok_or("the shipped catalog has a mobile ground unit")?;
        let unit = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(8, 8))?;
        let entity = world.entities.get_mut(&unit).ok_or("unit")?;
        assert_eq!(entity.weapon_target, None, "a new unit has chosen nothing");
        for order in every_order(SimVec2::from_i32(4, 4)) {
            entity.weapon_target = Some(EntityId(77));
            entity.commit_order(order.clone(), false);
            assert_eq!(entity.weapon_target, None, "{order:?} replaces the orders");
            entity.order_queue = [UnitOrder::AttackTarget {
                target: EntityId(98),
                origin: AttackOrigin::Explicit,
            }]
            .into_iter()
            .collect();
            entity.weapon_target = Some(EntityId(77));
            entity.commit_order(order.clone(), true);
            assert_eq!(
                entity.weapon_target,
                Some(EntityId(77)),
                "{order:?} queued leaves the front order and its weapon target"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod teleport_tests {
    //! Plan `2026-11-24-weapon-validation` D-2 (P-3): [`Entity::teleport_to`] relocates the unit
    //! and counts one teleport per call, wrapping.
    use crate::test_support::world;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};

    fn walker() -> Result<super::Entity, Box<dyn std::error::Error>> {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = world(map, 5);
        let kind = world
            .catalog
            .archetypes()
            .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .map(|arch| arch.unit_type_id)
            .ok_or("the shipped catalog has a mobile ground archetype")?;
        let id = world.spawn_unit(kind, PlayerId(0), SimVec2::from_i32(4, 4))?;
        Ok(world
            .entities
            .remove(&id)
            .ok_or("the spawned unit exists")?)
    }

    #[test]
    fn a_teleport_relocates_the_unit_and_counts_once() -> Result<(), Box<dyn std::error::Error>> {
        let mut unit = walker()?;
        assert_eq!(unit.teleport_count, 0, "a new unit has never teleported");
        unit.velocity = SimVec2::from_i32(1, 0);
        unit.current_funnel_waypoints.push(SimVec2::from_i32(9, 9));
        let destination = SimVec2::from_i32(11, 7);
        unit.teleport_to(destination);
        assert_eq!(unit.position, destination);
        assert_eq!(unit.velocity, SimVec2::ZERO);
        assert_eq!(unit.current_funnel_waypoints, Vec::<SimVec2>::new());
        assert_eq!(unit.teleport_count, 1);
        unit.teleport_to(destination);
        assert_eq!(unit.teleport_count, 2, "every call counts, even in place");
        Ok(())
    }

    #[test]
    fn the_teleport_count_wraps() -> Result<(), Box<dyn std::error::Error>> {
        let mut unit = walker()?;
        unit.teleport_count = u16::MAX;
        unit.teleport_to(SimVec2::from_i32(2, 2));
        assert_eq!(unit.teleport_count, 0);
        Ok(())
    }
}

#[cfg(test)]
mod can_attack_tests {
    //! Plan `2026-11-24-attack-orders` D-2: `Entity::can_attack`, the one rule for "can this
    //! unit attack that unit" the simulation's admission and the client's share.
    use crate::catalog::UnitArchetype;
    use crate::test_support::world;
    use crate::SimWorld;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        AbilityEffectSpec, AttackCommand, AttributeFlags, EntityId, MoverType, PlayerId, SimVec2,
        StanceOrders,
    };

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn bare() -> Result<SimWorld, Box<dyn std::error::Error>> {
        Ok(world(
            GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?,
            5,
        ))
    }

    /// The first shipped archetype `pick` accepts.
    fn find(world: &SimWorld, pick: impl Fn(&UnitArchetype) -> bool) -> Option<UnitArchetype> {
        world.catalog.archetypes().find(|arch| pick(arch)).cloned()
    }

    /// Whether `unit` can attack `target`, by their own archetypes.
    fn attacks(world: &SimWorld, unit: EntityId, target: EntityId) -> bool {
        let unit = &world.entities[&unit];
        let target = &world.entities[&target];
        let (Ok(arch), Ok(target_arch)) = (
            world.catalog.get(unit.unit_type_id),
            world.catalog.get(target.unit_type_id),
        ) else {
            return false;
        };
        unit.can_attack(arch, target, target_arch)
    }

    /// Self is refused, the owner is not read, and the plane and attribute flags of the target
    /// are: a ground-only armed walker attacks an enemy and an own ground unit, never itself or
    /// an air unit, and nothing its weapon's `target_flags` leave out.
    #[test]
    fn self_is_refused_owner_ignored_plane_and_flags_read() -> TestResult {
        let mut world = bare()?;
        let gunner = find(&world, |arch| {
            !arch.is_immobile_by_default()
                && arch.mover_type == MoverType::Ground
                && arch.air_weapon.is_none()
                && arch.weapon_against(MoverType::Ground, None).is_some()
                && arch.modes.is_empty()
        })
        .ok_or("a ground-only armed walker")?;
        let flyer = find(&world, |arch| arch.mover_type == MoverType::Air).ok_or("an air unit")?;
        let at = |x: i32| SimVec2::from_i32(x, 10);
        let unit = world.spawn_unit(gunner.unit_type_id, PlayerId(0), at(8))?;
        let own = world.spawn_unit(gunner.unit_type_id, PlayerId(0), at(10))?;
        let enemy = world.spawn_unit(gunner.unit_type_id, PlayerId(1), at(12))?;
        let enemy_air = world.spawn_unit(flyer.unit_type_id, PlayerId(1), at(14))?;
        assert!(attacks(&world, unit, enemy), "an enemy ground unit");
        assert!(
            attacks(&world, unit, own),
            "an own ground unit: the owner is not read"
        );
        assert!(!attacks(&world, unit, unit), "never itself");
        assert!(!attacks(&world, unit, enemy_air), "no weapon against air");

        let mut blind = gunner.clone();
        for weapon in [&mut blind.weapon, &mut blind.ground_weapon]
            .into_iter()
            .flatten()
        {
            weapon.target_flags = AttributeFlags::empty();
        }
        let shooter = &world.entities[&unit];
        let target = &world.entities[&enemy];
        assert!(
            !shooter.can_attack(&blind, target, &gunner),
            "target_flags that meet none of the target's attributes"
        );

        let mut scan_mover = gunner.clone();
        scan_mover.default_stance_orders = Some(StanceOrders {
            attack: AttackCommand::ScanMove,
            ..StanceOrders::ALL
        });
        assert!(
            !shooter.can_attack(&scan_mover, target, &gunner),
            "an armed stance whose A command is not Attack"
        );
        assert_eq!(
            shooter.settled_attack_command(&scan_mover),
            AttackCommand::ScanMove
        );
        assert_eq!(
            shooter.settled_attack_command(&gunner),
            AttackCommand::Attack
        );
        Ok(())
    }

    /// A unit armed only in its mode, with Attack in its default stance (the Lurker analogue,
    /// plan S7): it attacks nothing until it is burrowing, and from the start of the switch it
    /// attacks with the mode's replacement weapon, as the settled stance gives it.
    #[test]
    fn the_settled_replacement_weapon_is_used_while_switching() -> TestResult {
        let mut world = bare()?;
        let burrower = find(&world, |arch| {
            !arch.stance_armed(None)
                && !arch.is_immobile_by_default()
                && arch.modes.iter().any(|mode| arch.stance_armed(Some(mode)))
        })
        .ok_or("a unit armed only in its mode")?;
        let ground = find(&world, |arch| {
            !arch.is_immobile_by_default() && arch.mover_type == MoverType::Ground
        })
        .ok_or("a ground walker")?;
        let switch = burrower
            .abilities
            .iter()
            .position(|ability| matches!(ability.effect, AbilityEffectSpec::SwitchMode { .. }))
            .ok_or("its switch")?;
        let unit = world.spawn_unit(burrower.unit_type_id, PlayerId(0), SimVec2::from_i32(8, 8))?;
        let enemy = world.spawn_unit(ground.unit_type_id, PlayerId(1), SimVec2::from_i32(10, 8))?;
        assert_eq!(
            world.entities[&unit].settled_attack_command(&burrower),
            burrower.stance_orders(None).attack
        );
        assert!(
            !attacks(&world, unit, enemy),
            "no weapon in the default stance"
        );
        let mode = burrower
            .modes
            .iter()
            .find(|mode| burrower.stance_armed(Some(mode)))
            .ok_or("the arming mode")?;
        assert!(
            burrower
                .stance_weapon_against(Some(mode), MoverType::Ground)
                .is_some(),
            "the mode's replacement weapon"
        );
        assert!(burrower
            .stance_weapon_against(None, MoverType::Ground)
            .is_none());
        assert!(world.execute_ability(unit, u8::try_from(switch)?, None, None));
        assert!(
            world.entities[&unit].channeling_remaining_ticks > 0,
            "switching"
        );
        assert!(attacks(&world, unit, enemy), "the settled stance's weapon");
        assert_eq!(
            world.entities[&unit].settled_attack_command(&burrower),
            mode.orders.attack
        );
        Ok(())
    }
}

#[cfg(test)]
mod last_seen_tests {
    //! Plan `2026-11-24-weapon-validation` Amendment 2 (B-195): `MovementMemory::sight` records the
    //! last sighting of an attack order's target, marks it lost, and restarts the walk's progress
    //! only when the goal changes between the target and its last seen point.
    use super::{LastSeen, MovementMemory, SlotClaim};
    use sim_types::{EntityId, SimScalar, SimVec2};

    const TARGET: EntityId = EntityId(7);

    fn at(x: i32, y: i32) -> SimVec2 {
        SimVec2::from_i32(x, y)
    }

    /// A memory mid-chase: progress measured and a ring gap claimed.
    fn chasing() -> MovementMemory {
        MovementMemory {
            progress_best: Some(SimScalar::from_i32(3)),
            stalled_ticks: 4,
            slot: Some(SlotClaim {
                target: TARGET,
                point: at(1, 1),
            }),
            ..MovementMemory::default()
        }
    }

    #[test]
    fn a_target_never_seen_has_nothing_to_lose() {
        let mut memory = chasing();
        memory.sight(TARGET, None);
        assert_eq!(memory, chasing());
    }

    #[test]
    fn each_sighting_moves_the_point_and_a_loss_keeps_it() {
        let mut memory = chasing();
        memory.sight(TARGET, Some(at(5, 5)));
        memory.sight(TARGET, Some(at(6, 5)));
        assert_eq!(
            memory.last_seen,
            Some(LastSeen {
                target: TARGET,
                point: at(6, 5),
                lost: false
            })
        );
        assert_eq!(
            memory.lost_target_point(TARGET),
            None,
            "a seen target is chased"
        );
        assert_eq!(
            memory.progress_best,
            chasing().progress_best,
            "the chase goes on"
        );
        memory.sight(TARGET, None);
        memory.sight(TARGET, None);
        assert_eq!(memory.lost_target_point(TARGET), Some(at(6, 5)));
        assert_eq!(
            memory.lost_target_point(EntityId(8)),
            None,
            "keyed by the target"
        );
    }

    #[test]
    fn losing_or_finding_the_target_restarts_the_walk() {
        let mut memory = chasing();
        memory.sight(TARGET, Some(at(5, 5)));
        memory.sight(TARGET, None);
        assert_eq!(memory.progress_best, None);
        assert_eq!(memory.stalled_ticks, 0);
        assert_eq!(memory.slot, None);
        memory.progress_best = Some(SimScalar::ONE);
        memory.stalled_ticks = 2;
        memory.sight(TARGET, None);
        assert_eq!(
            memory.progress_best,
            Some(SimScalar::ONE),
            "still lost: no change"
        );
        memory.sight(TARGET, Some(at(9, 9)));
        assert_eq!(memory.progress_best, None, "found again");
        assert_eq!(memory.stalled_ticks, 0);
    }

    #[test]
    fn another_targets_sighting_replaces_the_record_and_a_restart_clears_it() {
        let mut memory = chasing();
        memory.sight(TARGET, Some(at(5, 5)));
        memory.sight(EntityId(8), None);
        assert_eq!(memory.last_seen.map(|seen| seen.target), Some(TARGET));
        memory.sight(EntityId(8), Some(at(2, 2)));
        assert_eq!(memory.last_seen.map(|seen| seen.target), Some(EntityId(8)));
        memory.restart();
        assert_eq!(memory.last_seen, None);
    }
}
