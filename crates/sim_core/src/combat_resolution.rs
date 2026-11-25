//! # Combat Resolution: Tick Phase 7
//!
//! Owns everything between "a weapon cycle advanced" and "damage landed": advancing each
//! entity's weapon state machine, dispatching discharged shots, flying projectiles, and
//! applying direct and area damage.
//!
//! ## How it fits
//! [`crate::combat`] holds the pure rules (damage formulas and the single-entity weapon state
//! machine). This module drives those rules over the whole [`SimWorld`] in tick order. It runs
//! after movement and collision, so shots see this tick's final positions, and before reaping,
//! so an entity killed here still exists for the remainder of the phase.
//!
//! ## Invariants Maintained
//! 1. Every direct hit goes through [`SimWorld::apply_weapon_hit`] and all area damage goes
//!    through [`SimWorld::apply_splash`], so a rule changes in one place.
//! 2. Shots are prepared in `EntityId` order, then applied in that same order; projectile
//!    impacts apply in descending launch-list order. Both orders are deterministic.
//! 3. No shot changes another entity's position, owner or lifecycle link during this phase.
//! 4. An attacker-centred area ([`SplashImpact::Attacker`]) is the only damage of its shot: no
//!    direct strike, so no weapon hit effect; its target is struck by the area like any other
//!    unit in it (plan `2026-11-23-splash-centres` D-1), or, when it shares no cliff level with
//!    the release position and the area says so, by the area's other-level strike, which is part of the area
//!    and passes the area's own filters (plan `2026-11-23-splash-cliff-levels` D-3).
//! 5. An area with a target radius condition opens only around a target whose collision radius
//!    is below it; otherwise it searches nothing and publishes nothing (D-3).
//! 6. An area whose `cliff` is `Same` strikes only units whose cliff span overlaps the span of
//!    a search point that reaches them: a ramp cell spans both its landings, and an area on the
//!    air plane, or a unit in the air, spans every level (owner answer Q0099 1(c), 2(b); plan
//!    `2026-11-24-owner-answers-cliff-and-vision` D-2). The level is read at the search point
//!    (Q0099 3(a)).
//! 7. A projectile whose target is lost lands at the last tracked point unless its weapon says
//!    `Fizzle` (plan `2026-11-23-vision-and-detection` D-1).
//! 8. A strike by a `Visible` weapon reveals its attacker to the struck unit's owner for the
//!    weapon's `reveal_ticks`, recorded before the strike's retaliation runs, so the response
//!    of the same tick sees it (plan `2026-11-23-vision-and-detection` D-5).
//! 9. A missile loses its target when the target teleports after the launch, unless its weapon
//!    clears `missile_validation.teleport`, and when the target fails the weapon's filters,
//!    unless its weapon clears `missile_validation.weapon`; a missile that does not validate the
//!    weapon strikes only a target that passes it at impact (plan `2026-11-24-weapon-validation`
//!    D-4).
//! 10. A strike by a `Snapshot` weapon on a unit whose owner does not see the attacker (and, if it
//!     is concealed, detects its cell) leaves that owner a snapshot of the attacker in its pose
//!     at the strike, in the unhashed presentation workspace, which no tick decision reads (plan
//!     `2026-11-24-attacker-snapshot` D-2, D-3). The reveal and the snapshot are recorded by one
//!     call, [`SimWorld::record_attack_presentation`], for an [`AttackCause`]: a weapon's strike
//!     (from both strike paths), a weapon missile's release (from [`SimWorld::launch_projectile`],
//!     with the weapon's launch visibility, toward the owner of the unit it is aimed at), and a
//!     spell's damage or unit-aimed missile (from the effect engine, with the node's visibility
//!     and the cast's caster). The rule does not ask the cause (plan
//!     `2026-11-27-launch-and-spell-visibility` D-2).
//! 11. A swing whose bound target fails the weapon's availability test has ended: it deals
//!     nothing, charges no cooldown and holds nothing. [`SimWorld::end_lost_swings`] applies the
//!     test after the vision update and before acquisition and movement, so no later phase of
//!     the tick reads a lost swing; the weapon phase applies the same test
//!     ([`SimWorld::swing_target_live`]) to a target lost during the tick's movement (plan
//!     `2026-11-25-wind-up-and-hold-facing` D-1, B-207).

use crate::catalog::UnitCatalog;
use crate::catalog::{SplashSpec, UnitArchetype, WeaponSpec};
use crate::combat::{
    charge_random_delay, fires_without_attack_order, mitigate_weapon_strike, raw_strike_damage,
    shares_combat_plane, splash_raw_damage, step_entity_weapon_tracking, within_band, Mandate,
    TrackingAim, WeaponTarget,
};
use crate::entity::{
    ActiveProjectile, Entity, PendingLine, PendingLineView, PendingStrikes, WeaponPhase,
};
use crate::weapon_timing::{resolve_attack_period, AttackRate, SimulationDiagnostic};
use crate::world::presentation::{
    mode_index, weapon_source, AttackSnapshotRecord, PresentationWorkspace, SnapshotPose,
};
use crate::world::SimWorld;
use sim_map::{CliffSpan, PlayerVisionMap, SimMap};
use sim_types::{
    AbilitySource, AttackVisibility, CombatEventVisual, EffectContext, EffectSource, EffectTarget,
    EntityId, LineView, MoverType, PerPlayer, PlayerId, PlayerUpgradeState,
    ProjectileTargetLossBehavior, SimAngle, SimScalar, SimVec2, SplashCliff, SplashImpact,
    SplashShape, StrikeEnds, Tick, UnitOrder, UnitTypeId, WeaponVisualKind,
};

/// Lowest impact distance used for flight timing, so a point-blank launch still takes a tick.
const MIN_FLIGHT_DISTANCE: f64 = 0.05;

/// Everything a weapon needs to know about a potential target, captured before mutation.
#[derive(Clone, Copy)]
struct TargetSnapshot {
    /// World position at the start of the weapon phase.
    position: SimVec2,
    /// Authored collision radius in world units.
    radius: SimScalar,
    /// Locomotion layer, retained even for cargo so weapon selection stays stable.
    mover: MoverType,
    /// Passes the damage gate of [`SimWorld::is_damageable`].
    damageable: bool,
    /// Requires detection before anyone may aim at it.
    cloaked: bool,
    /// For each player, whether the entity's own attacks reveal it to that player (B-55), as
    /// [`Entity::is_attack_revealed_to`] at the start of the weapon phase.
    attack_revealed: PerPlayer<bool>,
}

/// One discharged weapon waiting for dispatch.
struct PreparedShot {
    attacker: EntityId,
    target: EntityId,
    weapon: WeaponSpec,
    source_slug: String,
    origin: SimVec2,
    owner: PlayerId,
    /// Strikes of `weapon` this shot lands: all of them on a release whose strikes land
    /// together, one on a release that spaces them and on each later strike, and every strike
    /// left of a delayed volley on its due tick. A release whose first strike is delayed queues
    /// no shot.
    strikes: u8,
    /// The attacker's type and weapon, for the shot's renderer events.
    source: EffectSource,
}

/// Provenance of a hit, borrowed from a prepared shot or a landed projectile.
#[derive(Clone, Copy)]
struct HitSource<'a> {
    weapon: &'a WeaponSpec,
    slug: Option<&'a str>,
    owner: PlayerId,
    attacker: Option<EntityId>,
    origin: SimVec2,
    /// Strikes of `weapon` the hit applies to its target.
    strikes: u8,
    /// The attacker's type and weapon, for the hit's renderer events.
    effect: EffectSource,
}

/// What an effect that lands on, or is aimed at, a unit shows of its caster (plan
/// `2026-11-27-launch-and-spell-visibility` D-2): the effect's visibility (`StarCraft` II's
/// `Visibility` of `CEffectDamage` or `CEffectLaunchMissile`), the caster, and the caster's
/// owner. [`SimWorld::record_attack_presentation`] turns it into a reveal or a snapshot for the
/// owner of the unit struck or aimed at; it reads nothing else.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AttackCause {
    /// What the effect shows of its caster.
    visibility: AttackVisibility,
    /// The casting unit: the attacker of a weapon, the caster of a spell.
    attacker: Option<EntityId>,
    /// The caster's owner: the shot's owner, or the cast's.
    owner: PlayerId,
}

impl AttackCause {
    /// The damage of a weapon hit: `source.weapon.attack_visibility`.
    fn strike(source: HitSource<'_>) -> Self {
        Self {
            visibility: source.weapon.attack_visibility,
            attacker: source.attacker,
            owner: source.owner,
        }
    }

    /// A weapon missile's release: `shot.weapon.launch_visibility`.
    fn launch(shot: &PreparedShot) -> Self {
        Self {
            visibility: shot.weapon.launch_visibility,
            attacker: Some(shot.attacker),
            owner: shot.owner,
        }
    }

    /// A spell node's `visibility`, with the context's caster and its owner at the cast.
    pub(crate) const fn spell(visibility: AttackVisibility, context: &EffectContext) -> Self {
        Self {
            visibility,
            attacker: context.caster,
            owner: context.owner,
        }
    }
}

/// Where a direct hit landed, for the area damage and visuals that follow it.
#[derive(Clone, Copy)]
struct ResolvedHit {
    position: SimVec2,
    angle: SimAngle,
    /// Combat plane of the area damage this hit may cause.
    plane: MoverType,
    /// Collision radius of the struck target in world units; zero for a hit on a point.
    radius: SimScalar,
}

/// Where an area strikes: the plane it reaches, the direction its hits come from, and the
/// target it spares because a direct hit struck it already.
#[derive(Clone, Copy)]
struct AreaStrike {
    plane: MoverType,
    angle: SimAngle,
    primary: Option<EntityId>,
}

/// Outcome of advancing one projectile by a tick.
#[derive(Clone, Copy)]
enum ProjectileStep {
    /// Still travelling.
    Flying,
    /// Reached its target surface or impact coordinate.
    Impact,
    /// Fizzled or fuelled out without damage.
    Disjointed,
}

// PERFORMANCE RATIONALE:
// Why: This phase runs every tick and, in a large battle, touches every entity; rebuilding a
// map of target snapshots and a list of shots per tick is steady allocator traffic.
// What: Keep sorted snapshots and the shot, splash and impact lists in retained buffers.
// Trade-off: Buffers stay at their high-water mark and must be taken out of the workspace
// while `SimWorld` methods run, in exchange for no per-tick heap growth in this bookkeeping.
/// Retained buffers for phase 7.
#[derive(Default)]
pub(crate) struct CombatWorkspace {
    /// Sorted by `EntityId`, because they are built from an ordered map walk.
    snapshots: Vec<(EntityId, TargetSnapshot)>,
    shots: Vec<PreparedShot>,
    /// Each struck entity with the raw damage it takes, before its barrier and armour.
    splash_hits: Vec<(EntityId, SimScalar)>,
    /// Centres of the current splash's searches (one for a circle, one per offset for a line).
    splash_points: Vec<SimVec2>,
    // PERFORMANCE RATIONALE:
    // Why: An area kept to its level compares each unit in reach with the level of every search
    // point, and a strike must not allocate.
    // What: The cliff spans of the current strike's search points, read once per strike beside
    // the points, in the same order.
    // Trade-off: One more buffer held at its high-water mark (a line's point count).
    /// Cliff span of each of the current strike's search points, for an area kept to its level.
    splash_levels: Vec<CliffSpan>,
    /// Units with pending line searches this tick, in `EntityId` order.
    line_owners: Vec<EntityId>,
    /// Units whose wind-up [`SimWorld::end_lost_swings`] ends this tick, in `EntityId` order.
    lost_swings: Vec<EntityId>,
    finished_projectiles: Vec<(usize, ProjectileStep)>,
}

impl CombatWorkspace {
    fn snapshot(&self, id: EntityId) -> Option<TargetSnapshot> {
        self.snapshots
            .binary_search_by_key(&id, |entry| entry.0)
            .ok()
            .map(|index| self.snapshots[index].1)
    }
}

/// A weapon as it acts this tick: its catalog (or siege) spec with the range and the attack
/// period that upgrades and attack-rate buffs resolve to.
///
/// Produced only by [`SimWorld::resolved_weapon`], which both the combat phase and the render
/// snapshot call, so the cycle combat runs and the cycle the snapshot publishes cannot differ.
// PERFORMANCE RATIONALE:
// Why: The render snapshot reads the period of every swinging entity each tick, and a cloned
// `WeaponSpec` allocates (its `name` is a `String`).
// What: Borrow the spec and carry the two resolved values beside it; only combat, which hands
// an owned spec to the state machine and to a prepared shot, clones it (`to_spec`), as before.
// Trade-off: Two fields that shadow the spec's own, read through this type rather than the spec.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedWeapon<'a> {
    /// The authored weapon: siege override, ground, generic or air spec.
    pub(crate) spec: &'a WeaponSpec,
    /// Maximum range after the owner's range upgrades, in world units.
    pub(crate) max_range: SimScalar,
    /// Attack period after cooldown upgrades and attack-rate buffs, in ticks; at least 1.
    pub(crate) period_ticks: u16,
}

impl ResolvedWeapon<'_> {
    /// The spec with the resolved range and period written in, as the weapon state machine and
    /// a prepared shot take it.
    pub(crate) fn to_spec(self) -> WeaponSpec {
        WeaponSpec {
            max_range: self.max_range,
            period_ticks: self.period_ticks,
            ..self.spec.clone()
        }
    }
}

/// The world state the weapon phase reads beside the entities it mutates.
struct StrikeContext<'a> {
    catalog: &'a UnitCatalog,
    upgrades: &'a PerPlayer<PlayerUpgradeState>,
    map: &'a SimMap,
    vision_maps: &'a PerPlayer<PlayerVisionMap>,
}

impl StrikeContext<'_> {
    /// Whether `viewer`'s weapons may strike the entity `snapshot` describes: the weapon filter
    /// of [`SimWorld::passes_weapon_filters`] on the snapshot. It passes the damage gate, is
    /// seen by `viewer` (its cell `Visible`, or attack-revealed) and, if cloaked, is revealed
    /// to `viewer` (plan `2026-11-24-weapon-validation` D-3).
    fn is_available(&self, viewer: PlayerId, snapshot: TargetSnapshot) -> bool {
        let revealed = viewer
            .slot()
            .is_some_and(|slot| snapshot.attack_revealed[slot]);
        snapshot.damageable
            && SimWorld::sees_point(
                self.map,
                self.vision_maps,
                viewer,
                snapshot.position,
                revealed,
            )
            && (!snapshot.cloaked
                || SimWorld::is_cloak_revealed(
                    self.map,
                    self.vision_maps,
                    viewer,
                    snapshot.position,
                    revealed,
                ))
    }
}

/// The cliff levels a position counts as for an area kept to its level, as the plane or mover
/// type `mover` reads them: every level in the air ([`CliffSpan::ALL`]: an air blast and an air
/// unit ignore cliff levels, owner answer Q0099 2(b)), and otherwise the ground's span under
/// `pos` (a ramp cell spans both its landings, Q0099 1(c)). A `CliffWalker` stands on the ground
/// and keeps the ground's span (plan `2026-11-24-owner-answers-cliff-and-vision` D-2, §14 F-2).
fn level_span(map: &SimMap, pos: SimVec2, mover: MoverType) -> CliffSpan {
    if mover == MoverType::Air {
        CliffSpan::ALL
    } else {
        map.cliff_span_at(pos)
    }
}

/// The pose of `attacker` (of archetype `arch`) a snapshot shows: where it stands, its hull and
/// turret facings, its settled mode and its health and shield shares, in fixed point (plan
/// `2026-11-24-attacker-snapshot` D-4).
pub(crate) fn snapshot_pose(attacker: &Entity, arch: &UnitArchetype) -> SnapshotPose {
    let share = |value: SimScalar, max: SimScalar| {
        crate::world::snapshot::fraction(value, max).clamp(SimScalar::ZERO, SimScalar::ONE)
    };
    SnapshotPose {
        unit_type: attacker.unit_type_id,
        owner: attacker.owner,
        position: attacker.position,
        heading: attacker.heading,
        turret: attacker.turret_angle,
        mode: attacker
            .settled_mode(arch)
            .and_then(|mode| mode_index(arch, &mode.mode_name)),
        hp_fraction: share(attacker.hp, attacker.max_hp),
        shield_fraction: share(attacker.shields, arch.max_shields),
    }
}

/// Direction a hit arrives from, as stored for hull-impact rendering.
fn hit_angle(origin: SimVec2, target: SimVec2) -> SimAngle {
    SimAngle::atan2(origin.y - target.y, origin.x - target.x)
}

/// The line a line splash's searches lie on, for their renderer events: the point its offsets
/// are measured from, that point moved by the largest offset, and the spacing of its first two
/// offsets, in world units; `None` for a circle. `attacker`, `impact` and `direction` are those of
/// [`SplashSpec::line_frame`].
fn splash_line_view(
    splash: &SplashSpec,
    attacker: SimVec2,
    impact: SimVec2,
    direction: SimVec2,
) -> Option<(SimVec2, SimVec2, SimScalar)> {
    splash
        .line_frame(attacker, impact, direction)
        .map(|(line, origin, axis)| {
            let far = line
                .offsets
                .iter()
                .copied()
                .max()
                .unwrap_or(SimScalar::ZERO);
            let spacing = match line.offsets.as_slice() {
                [first, second, ..] => (*second - *first).abs(),
                _ => SimScalar::ZERO,
            };
            (origin, origin + axis * far, spacing)
        })
}

/// The radii in world units of `splash`'s innermost, middle and outermost zones, as a
/// `SplashVisual` publishes them; with fewer than three zones the middle is the innermost.
fn splash_zone_radii(splash: &SplashSpec) -> [f32; 3] {
    let radius = |index: usize| {
        splash
            .zones
            .get(index)
            .map_or(0.0, |zone| zone.radius.to_f32())
    };
    let last = splash.zones.len().saturating_sub(1);
    [radius(0), radius(last / 2), splash.reach().to_f32()]
}

/// Lands one strike of `weapon` carrying `raw` damage (before armour) on `target`.
///
/// The target's barrier, if its archetype has one, absorbs first (B-41). What it lets through
/// is mitigated by shields and armour and applied. A strike the barrier absorbs whole deals
/// nothing: no 0.5 minimum, no hit flash, and the regeneration delays keep running. A barrier
/// that absorbs any of the strike is noted in `presentation` for the render snapshot.
fn strike_through_barrier(
    target: &mut Entity,
    target_arch: &UnitArchetype,
    target_upgrades: &PlayerUpgradeState,
    weapon: &WeaponSpec,
    raw: SimScalar,
    angle: SimAngle,
    presentation: &mut PresentationWorkspace,
) {
    let passed = target
        .barrier
        .absorb(target_arch.capabilities.barrier.as_ref(), raw);
    if passed < raw {
        presentation.record_barrier_hit(target.id);
    }
    let raw = passed;
    if raw <= SimScalar::ZERO {
        return;
    }
    let resolution = mitigate_weapon_strike(
        raw,
        weapon,
        target_arch,
        target_upgrades,
        target.shields,
        target.hp,
    );
    target.apply_damage(resolution.shield_damage, resolution.life_damage, angle);
}

impl SimWorld {
    /// Phase 7: Advance weapon state machines, fire attacks, and step projectiles.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn step_weapons_and_projectiles(&mut self) {
        self.capture_target_snapshots();
        self.advance_pending_lines();
        self.advance_weapon_cycles();

        let mut shots = std::mem::take(&mut self.combat.shots);
        for shot in shots.drain(..) {
            self.dispatch_shot(shot);
        }
        self.combat.shots = shots;

        self.advance_projectiles();
    }

    /// Counts down every unit's pending line searches ([`Entity::pending_line`], B-85) and lands
    /// the ones that fall due, in `EntityId` order.
    ///
    /// It runs before this tick's releases, so a line released on a tick waits its whole first
    /// period. The line ends when its attacker dies,
    /// boards a transport, starts a mode change (channeling) or no longer resolves a weapon with
    /// an area against the line's plane; each search uses the weapon as it resolves now.
    fn advance_pending_lines(&mut self) {
        let mut owners = std::mem::take(&mut self.combat.line_owners);
        owners.clear();
        owners.extend(
            self.entities
                .iter()
                .filter(|(_, entity)| entity.pending_line.is_some())
                .map(|(&id, _)| id),
        );
        for &id in &owners {
            let Some(entity) = self.entities.get_mut(&id) else {
                continue;
            };
            let Some(mut pending) = entity.pending_line.take() else {
                continue;
            };
            if pending.ticks_until_next > 1 {
                pending.ticks_until_next -= 1;
                entity.pending_line = Some(pending);
                continue;
            }
            let Some((weapon, slug)) = self.pending_line_weapon(id, pending.plane) else {
                continue;
            };
            let Some(splash) = weapon.splash.as_ref() else {
                continue;
            };
            if pending.points.is_empty() {
                continue;
            }
            let view = pending.view;
            let index = view
                .count
                .saturating_sub(u8::try_from(pending.points.len()).unwrap_or(u8::MAX));
            let point = pending.points.remove(0);
            let Some(owner) = self.entities.get(&id).map(|entity| entity.owner) else {
                continue;
            };
            let source = HitSource {
                weapon: &weapon,
                slug: Some(&slug),
                owner,
                attacker: Some(id),
                origin: point,
                strikes: 1,
                effect: view.effect,
            };
            let area = AreaStrike {
                plane: pending.plane,
                angle: pending.angle,
                primary: Some(pending.primary),
            };
            self.emit_search_visuals(
                source,
                splash,
                pending.plane,
                view.line,
                &[point],
                (index, view.count),
            );
            self.strike_area(source, splash, area, &[point], Some(&mut pending.struck));
            let period = match &splash.shape {
                SplashShape::Line(line) => line.period_ticks,
                SplashShape::Circle => 0,
            };
            if !pending.points.is_empty() && period > 0 {
                pending.ticks_until_next = period;
                if let Some(entity) = self.entities.get_mut(&id) {
                    entity.pending_line = Some(pending);
                }
            }
        }
        self.combat.line_owners = owners;
    }

    /// The weapon with which unit `id` lands its next pending line search against `plane`, with
    /// its archetype's slug, or `None` when the line ends (see
    /// [`Self::advance_pending_lines`]).
    fn pending_line_weapon(&self, id: EntityId, plane: MoverType) -> Option<(WeaponSpec, String)> {
        let entity = self.entities.get(&id)?;
        if !entity.is_alive() || entity.is_hidden() || entity.channeling_remaining_ticks > 0 {
            return None;
        }
        let rate = entity.attack_rate_multiplier().ok()?;
        let arch = self.catalog.get(entity.unit_type_id).ok()?;
        let weapon =
            Self::resolved_weapon(&self.player_upgrades, entity, arch, plane, rate)?.to_spec();
        Some((weapon, arch.slug.clone()))
    }

    /// Records every entity's targeting data so weapons can read targets while mutating
    /// their owners.
    fn capture_target_snapshots(&mut self) {
        let snapshots = &mut self.combat.snapshots;
        snapshots.clear();
        for (&id, entity) in &self.entities {
            snapshots.push((id, Self::target_snapshot(&self.catalog, entity)));
        }
    }

    /// The targeting data of `entity` as a weapon reads it now: position, collision radius and
    /// plane from its archetype in `catalog` (half a world unit on the ground for a type the
    /// catalog lacks), the damage gate, concealment and the per-player attack reveal. The one
    /// constructor of [`TargetSnapshot`], shared by the weapon phase and
    /// [`Self::end_lost_swings`], so both judge a target from the same facts.
    fn target_snapshot(catalog: &UnitCatalog, entity: &Entity) -> TargetSnapshot {
        let (radius, mover) = catalog
            .get(entity.unit_type_id)
            .map_or((SimScalar::HALF, MoverType::Ground), |a| {
                (a.collision_radius, a.mover_type)
            });
        // Retain authored weapon-selection metadata, but never expose cargo
        // coordinates to aim/damage. Air-only windup timing must not silently
        // switch to the missing-target Ground fallback when a target boards.
        TargetSnapshot {
            position: entity.position,
            radius,
            mover,
            damageable: Self::is_damageable(entity),
            cloaked: entity.is_concealed(),
            attack_revealed: PerPlayer::from_fn(|slot| entity.is_attack_revealed_to(slot.id())),
        }
    }

    /// Whether `attacker`'s weapon may still work against the target `snapshot` describes: the
    /// weapon's availability test ([`StrikeContext::is_available`]: damageable, seen and, if
    /// concealed, detected by `attacker`'s player). For the target a swing is bound to this is
    /// the statement of the rule that a swing whose target can no longer be struck has ended
    /// (plan `2026-11-25-wind-up-and-hold-facing` D-1; 5.0.14 `CAbilAttack` `AcquireFilters`
    /// `Dead`, `CWeapon` `TargetFilters Visible`): the weapon phase and
    /// [`Self::end_lost_swings`] both ask it.
    fn swing_target_live(
        context: &StrikeContext<'_>,
        attacker: &Entity,
        snapshot: TargetSnapshot,
    ) -> bool {
        context.is_available(attacker.owner, snapshot)
    }

    /// Whether the weapon phase steps `entity`'s weapon this tick: it is alive, completed, not
    /// hidden, not channelling a mode change and not locked after a cast. A weapon it does not
    /// step keeps its phase, cooldown included.
    fn steps_weapon(entity: &Entity) -> bool {
        entity.is_alive()
            && entity.channeling_remaining_ticks == 0
            && !entity.is_action_locked()
            && !entity.is_hidden()
            && entity.is_completed
    }

    /// Ends every wind-up whose bound target is gone or fails the weapon's availability test
    /// (`swing_target_live`): the weapon returns to `Ready` with no shot, no cooldown
    /// charged and no random delay drawn, as the weapon phase ends such a swing. Orders,
    /// targets and the cooldown are left to acquisition and the weapon phase.
    ///
    /// Runs in [`Self::step_into`] after the vision update and before acquisition (plan
    /// `2026-11-25-wind-up-and-hold-facing` D-1, B-207): the dead were reaped at the end of the
    /// last tick and this tick's vision is known, so acquisition, movement (which would hold the
    /// unit for its swing, `wind_up_holds`) and the weapon phase all read the swing as
    /// ended, and the unit may start its next swing this tick. `StarCraft` II 5.0.14 cancels an
    /// attack whose target dies before the damage point. A target lost during this tick's
    /// movement is still the weapon phase's to judge. Only units whose weapon the weapon phase
    /// steps (`steps_weapon`) are judged, in `EntityId` order; nothing is random.
    pub fn end_lost_swings(&mut self) {
        let mut lost = std::mem::take(&mut self.combat.lost_swings);
        lost.clear();
        let context = StrikeContext {
            catalog: &self.catalog,
            upgrades: &self.player_upgrades,
            map: &self.map,
            vision_maps: &self.vision_maps,
        };
        for (&id, entity) in &self.entities {
            let WeaponPhase::Windup { target_id, .. } = entity.weapon_phase else {
                continue;
            };
            let live = self.entities.get(&target_id).is_some_and(|target| {
                Self::swing_target_live(
                    &context,
                    entity,
                    Self::target_snapshot(&self.catalog, target),
                )
            });
            if !live && Self::steps_weapon(entity) {
                lost.push(id);
            }
        }
        for id in &lost {
            if let Some(entity) = self.entities.get_mut(id) {
                entity.weapon_phase = WeaponPhase::Ready;
            }
        }
        self.combat.lost_swings = lost;
    }

    /// Advances each armed entity's weapon state machine, queueing the shots that discharge.
    fn advance_weapon_cycles(&mut self) {
        let combat = &mut self.combat;
        let rng = &mut self.rng;
        let context = StrikeContext {
            catalog: &self.catalog,
            upgrades: &self.player_upgrades,
            map: &self.map,
            vision_maps: &self.vision_maps,
        };
        for (id, entity) in &mut self.entities {
            let attack_mult = match entity.attack_rate_multiplier() {
                Ok(rate) => rate,
                Err(source) => {
                    self.tick_diagnostics
                        .push(SimulationDiagnostic::InvalidBuffAttackRate {
                            tick: Tick(self.current_tick),
                            entity: *id,
                            source,
                        });
                    continue;
                }
            };
            Self::advance_pending_strikes(&context, combat, *id, entity, attack_mult);
            if !Self::steps_weapon(entity) {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };

            // A swing stays bound to the target it started on. Otherwise the orders decide
            // (plan `2026-11-23-turret-wind-up` D-2).
            let ordered = Self::ordered_target(&context, combat, entity, arch, attack_mult);
            let mandate = ordered.map_or(Mandate::AttackOrder, |(_, mandate)| mandate);
            let target_opt_id = if let WeaponPhase::Windup { target_id, .. } = entity.weapon_phase {
                Some(target_id)
            } else {
                ordered.map(|(target, _)| target)
            };

            let target_snapshot = target_opt_id.and_then(|tid| combat.snapshot(tid));
            let target_mover =
                target_snapshot.map_or_else(|| Self::idle_weapon_plane(entity, arch), |t| t.mover);

            let Some((weapon, source)) = Self::resolved_weapon(
                &self.player_upgrades,
                entity,
                arch,
                target_mover,
                attack_mult,
            )
            .map(|resolved| {
                let source = weapon_source(arch, entity, resolved.spec);
                (resolved.to_spec(), source)
            }) else {
                // A target that vanished mid-swing may leave no weapon to resolve here.
                if matches!(entity.weapon_phase, WeaponPhase::Windup { .. }) {
                    entity.weapon_phase = WeaponPhase::Ready;
                }
                continue;
            };

            let target_info = target_opt_id.and_then(|id| {
                let snapshot = target_snapshot?;
                Self::swing_target_live(&context, entity, snapshot).then_some(WeaponTarget {
                    id,
                    position: snapshot.position,
                    combined_radius: snapshot.radius + arch.collision_radius,
                })
            });

            let tracking = Self::tracking_aim(
                &context,
                combat,
                entity,
                arch,
                self.tracked_targets.get(*id),
            );
            if step_entity_weapon_tracking(entity, &weapon, target_info, mandate, tracking) {
                charge_random_delay(entity, &weapon, rng);
                if let Some(target) = target_info {
                    let strikes = Self::begin_strikes(entity, &weapon, target.id, target_mover);
                    if strikes > 0 {
                        combat.shots.push(PreparedShot {
                            attacker: *id,
                            target: target.id,
                            weapon,
                            source_slug: arch.slug.clone(),
                            origin: entity.position,
                            owner: entity.owner,
                            strikes,
                            source,
                        });
                    }
                }
            }
        }
    }

    /// Where `entity`'s turret aims for the enemy acquisition chose for it to track
    /// (`tracked`, from [`SimWorld::tracked_targets`], plan `2026-11-26-turret-tracking-weapon`
    /// D-4, Amendment 1): that enemy's
    /// position, while it lives and is available to the unit's weapon (the swing's test,
    /// [`Self::swing_target_live`]) and its plane resolves to a turreted weapon whose turret
    /// tracks any enemy, with that weapon's turret rate. `None` otherwise; the weapon step
    /// decides whether the turret is free to follow it.
    fn tracking_aim(
        context: &StrikeContext<'_>,
        combat: &CombatWorkspace,
        entity: &Entity,
        arch: &UnitArchetype,
        tracked: Option<EntityId>,
    ) -> Option<TrackingAim> {
        let snapshot = combat.snapshot(tracked?)?;
        let weapon = arch.weapon_against(snapshot.mover, entity.active_siege_weapon())?;
        (weapon.turret_tracks_any_enemy
            && weapon.has_turret
            && Self::swing_target_live(context, entity, snapshot))
        .then_some(TrackingAim {
            position: snapshot.position,
            turn_rate: weapon.turret_turn_rate,
        })
    }

    /// The target `entity` (of archetype `arch`, its attack rate `attack_rate`) works against
    /// inside an attack order on `target` (plan `2026-11-24-fire-rules` D-4, B-180).
    ///
    /// The order's target whenever the weapon can work against it this tick: it is available to
    /// the weapon ([`StrikeContext::is_available`], which asks for vision) and lies in the
    /// engagement band, with no slop. Otherwise the weapon's own target, which acquisition keeps
    /// only for a weapon that may fire at other targets and, while the unit chases, fires while
    /// moving ([`Entity::weapon_target`]), with [`Mandate::OwnInitiative`]: the target is not the
    /// order's, so a hull weapon would not turn the hull. With neither, the order's target, as
    /// before: the weapon idles until it can work against it.
    fn attack_order_target(
        context: &StrikeContext<'_>,
        combat: &CombatWorkspace,
        entity: &Entity,
        arch: &UnitArchetype,
        attack_rate: AttackRate,
        target: EntityId,
    ) -> (EntityId, Mandate) {
        let ordered = (target, Mandate::AttackOrder);
        let Some(own) = entity.weapon_target else {
            return ordered;
        };
        if Self::can_work(context, combat, entity, arch, attack_rate, target) {
            return ordered;
        }
        let licensed = combat.snapshot(own).is_some_and(|snapshot| {
            arch.weapon_against(snapshot.mover, entity.active_siege_weapon())
                .is_some_and(|weapon| weapon.fires_at_other_targets)
        });
        if licensed {
            (own, Mandate::OwnInitiative)
        } else {
            ordered
        }
    }

    /// Whether `entity` (of archetype `arch`) can work against `target` this tick: the weapon it
    /// would use against the target's plane (range resolved for its owner's upgrades) is
    /// available to it and the target lies in its engagement band, with no slop.
    fn can_work(
        context: &StrikeContext<'_>,
        combat: &CombatWorkspace,
        entity: &Entity,
        arch: &UnitArchetype,
        attack_rate: AttackRate,
        target: EntityId,
    ) -> bool {
        let Some(snapshot) = combat.snapshot(target) else {
            return false;
        };
        let Some(weapon) =
            Self::resolved_weapon(context.upgrades, entity, arch, snapshot.mover, attack_rate)
        else {
            return false;
        };
        context.is_available(entity.owner, snapshot)
            && within_band(
                entity.position.distance(snapshot.position),
                weapon.spec.min_range,
                weapon.max_range,
                snapshot.radius + arch.collision_radius,
            )
    }

    /// The target `entity` (of archetype `arch`) works against when it is not mid-swing, and why
    /// (plan `2026-11-23-turret-wind-up` D-2, B-90). An attack order names its target; a holding
    /// unit works against its `current_target` (`StarCraft` II's Hold Position acquires for it);
    /// both are an attack order's [`Mandate`]. Otherwise only a weapon whose data lets it fire
    /// outside an attack order ([`fires_without_attack_order`], the weapon `entity` would use
    /// against the target's plane) works against the `current_target` acquisition tracks for it
    /// ([`Mandate::OwnInitiative`]); every other weapon works against nothing. The mount plays no
    /// part. Inside an attack order, a weapon that may fire at other targets works against its
    /// own target while it cannot work against the order's ([`Self::attack_order_target`]).
    fn ordered_target(
        context: &StrikeContext<'_>,
        combat: &CombatWorkspace,
        entity: &Entity,
        arch: &UnitArchetype,
        attack_rate: AttackRate,
    ) -> Option<(EntityId, Mandate)> {
        if let Some(UnitOrder::AttackTarget { target, .. }) = entity.order_queue.front() {
            return Some(Self::attack_order_target(
                context,
                combat,
                entity,
                arch,
                attack_rate,
                *target,
            ));
        }
        let current = entity.current_target?;
        if entity.is_on_hold() {
            return Some((current, Mandate::AttackOrder));
        }
        let mover = combat.snapshot(current)?.mover;
        let weapon = arch.weapon_against(mover, entity.active_siege_weapon())?;
        fires_without_attack_order(weapon, entity.order_walks())
            .then_some((current, Mandate::OwnInitiative))
    }

    /// Starts the strikes of an attack `entity` releases with `weapon` at `target` on `plane`,
    /// returning how many land on this release tick.
    ///
    /// A weapon whose first strike is delayed (`first_strike_delay_ticks` above 0) lands none
    /// now: every strike waits in [`Entity::pending_strikes`], the first falling due that many
    /// ticks after the release and each validated when it does, as `StarCraft` II validates each
    /// period of a persistent on its own (plan `2026-11-23-strike-delays` D-2, B-81). Otherwise
    /// a weapon whose strikes land together (`strike_interval_ticks` 0, or a single strike)
    /// lands them all now, and a weapon that spaces them lands the first now and keeps the rest
    /// pending, one every interval (B-2). A burst still pending is replaced; catalog validation
    /// ends every burst within its attack period, so only a period shortened below the burst
    /// (by upgrades or buffs) can do that.
    fn begin_strikes(
        entity: &mut Entity,
        weapon: &WeaponSpec,
        target: EntityId,
        plane: MoverType,
    ) -> u8 {
        if weapon.first_strike_delay_ticks > 0 {
            entity.pending_strikes = Some(PendingStrikes {
                target_id: target,
                plane,
                remaining: weapon.attacks_count,
                ticks_until_next: weapon.first_strike_delay_ticks,
                channeled: weapon.channeled,
            });
            return 0;
        }
        if weapon.strike_interval_ticks == 0 || weapon.attacks_count < 2 {
            entity.pending_strikes = None;
            return weapon.attacks_count;
        }
        entity.pending_strikes = Some(PendingStrikes {
            target_id: target,
            plane,
            remaining: weapon.attacks_count - 1,
            ticks_until_next: weapon.strike_interval_ticks,
            channeled: weapon.channeled,
        });
        1
    }

    /// Counts down the pending strikes of `entity` (`id`) and queues the strikes that fall due:
    /// one of a spaced burst, or every strike left of a delayed volley (`strike_interval_ticks`
    /// 0), which lands whole as on a release.
    ///
    /// The burst ends when the attacker dies or boards a transport, when an explicit order ends a
    /// channeled burst ([`Entity::pending_strikes`], Q0058), or when it no longer has the
    /// weapon that released it. A strike that falls due lands only when its target is available
    /// to the attacker, still on the weapon's plane, and within the weapon's
    /// `followup_max_distance` (centre to centre) if it names one; otherwise that strike is lost
    /// and the burst goes on (`StarCraft` II validates each period's effect on its own).
    fn advance_pending_strikes(
        context: &StrikeContext<'_>,
        combat: &mut CombatWorkspace,
        id: EntityId,
        entity: &mut Entity,
        attack_rate: AttackRate,
    ) {
        let Some(pending) = entity.pending_strikes.take() else {
            return;
        };
        if !entity.is_alive() || entity.is_hidden() {
            return;
        }
        if pending.ticks_until_next > 1 {
            entity.pending_strikes = Some(PendingStrikes {
                ticks_until_next: pending.ticks_until_next - 1,
                ..pending
            });
            return;
        }
        let Some(arch) = context.catalog.get(entity.unit_type_id).ok() else {
            return;
        };
        let Some((weapon, source)) =
            Self::resolved_weapon(context.upgrades, entity, arch, pending.plane, attack_rate).map(
                |resolved| {
                    let source = weapon_source(arch, entity, resolved.spec);
                    (resolved.to_spec(), source)
                },
            )
        else {
            return;
        };
        let strikes = if weapon.strike_interval_ticks == 0 {
            pending.remaining
        } else {
            1
        };
        let remaining = pending.remaining.saturating_sub(strikes);
        if remaining > 0 && weapon.strike_interval_ticks > 0 {
            entity.pending_strikes = Some(PendingStrikes {
                remaining,
                ticks_until_next: weapon.strike_interval_ticks,
                ..pending
            });
        }
        let Some(target) = combat.snapshot(pending.target_id) else {
            return;
        };
        let in_reach = weapon
            .followup_max_distance
            .is_none_or(|limit| entity.position.distance(target.position) <= limit);
        if target.mover == pending.plane && in_reach && context.is_available(entity.owner, target) {
            combat.shots.push(PreparedShot {
                attacker: id,
                target: pending.target_id,
                weapon,
                source_slug: arch.slug.clone(),
                origin: entity.position,
                owner: entity.owner,
                strikes,
                source,
            });
        }
    }

    /// The plane whose weapon steps the cycle of `entity` while it has no target: the ground,
    /// or the air for a unit armed only against air.
    ///
    /// A weapon's cooldown and backswing run on every tick whether or not the unit has a target,
    /// as in `StarCraft` II. Resolving against the ground alone found no weapon for an air-only
    /// unit, which then skipped its weapon step and froze its cooldown and backswing until it
    /// next had a target (plan `2026-11-05-sc2-deviation-fixes` B-79).
    fn idle_weapon_plane(entity: &Entity, arch: &UnitArchetype) -> MoverType {
        if arch
            .weapon_against(MoverType::Ground, entity.active_siege_weapon())
            .is_some()
        {
            MoverType::Ground
        } else {
            MoverType::Air
        }
    }

    /// The weapon `entity` (of archetype `arch`) uses against a target on `target_mover`'s
    /// plane, with range and period resolved for its owner's upgrades and `attack_rate`
    /// (the entity's combined buff rate, [`Entity::attack_rate_multiplier`]).
    ///
    /// The one place a weapon's acting range and period are decided: the combat phase and the
    /// render snapshot both call it (plan `2026-11-04-battle-flipbook-animation` D-7). Takes the
    /// upgrade array rather than `&self` so callers holding `entities` mutably can use it.
    /// Returns `None` when the archetype has no weapon against that plane.
    pub(crate) fn resolved_weapon<'a>(
        upgrades: &PerPlayer<PlayerUpgradeState>,
        entity: &'a Entity,
        arch: &'a UnitArchetype,
        target_mover: MoverType,
        attack_rate: AttackRate,
    ) -> Option<ResolvedWeapon<'a>> {
        let spec = arch.weapon_against(target_mover, entity.active_siege_weapon())?;
        // Statuses change the range after upgrades (plan `2026-11-10-spellcaster-mechanics`
        // §6.4); neutral modifiers leave it as it is.
        let max_range = entity.status_modifiers().weapon_range(
            Self::effective_range_from_upgrades(upgrades, spec, &arch.slug, entity.owner),
            spec.melee,
        );
        let delta = Self::upgrades_of(upgrades, entity.owner)
            .unit_stat_modifiers
            .get(&arch.slug)
            .map_or(0, |modifiers| modifiers.attack_cooldown_delta);
        Some(ResolvedWeapon {
            spec,
            max_range,
            period_ticks: resolve_attack_period(spec.period_ticks, delta, attack_rate),
        })
    }

    /// Launches or instantly resolves one discharged shot.
    ///
    /// The prepared shot retains actual source provenance. Earlier attacks may damage this
    /// shooter or change its orders, but cannot move, change the owner of, or reap it before
    /// this dispatch phase finishes.
    fn dispatch_shot(&mut self, shot: PreparedShot) {
        let attacker = shot.attacker;
        let suicide = shot.weapon.suicide_on_attack;
        if let Some(speed) = shot.weapon.projectile_speed {
            self.launch_projectile(shot, speed);
        } else {
            self.resolve_instant_shot(&shot);
        }
        if suicide {
            if let Some(attacker) = self.entities.get_mut(&attacker) {
                attacker.hp = SimScalar::ZERO;
                attacker.shields = SimScalar::ZERO;
            }
        }
    }

    /// The drawn radii of `shot`'s attacker and target ([`UnitArchetype::drawn_radius`]), world
    /// units, looked up as [`Self::launch_projectile`] looks up their range radii.
    fn drawn_radii(&self, shot: &PreparedShot) -> (SimScalar, SimScalar) {
        let attacker = self
            .catalog
            .get(
                self.entities
                    .get(&shot.attacker)
                    .map_or(UnitTypeId(0), |a| a.unit_type_id),
            )
            .map_or(SimScalar::HALF, UnitArchetype::drawn_radius);
        let target = self
            .entities
            .get(&shot.target)
            .and_then(|t| self.catalog.get(t.unit_type_id).ok())
            .map_or(SimScalar::HALF, UnitArchetype::drawn_radius);
        (attacker, target)
    }

    /// Spawns the projectile for `shot` and emits its renderer event, then records what the
    /// release shows of the attacker to the owner of the unit it is aimed at (`StarCraft` II's
    /// `CEffectLaunchMissile` `Visibility`, plan `2026-11-27-launch-and-spell-visibility` D-2).
    fn launch_projectile(&mut self, shot: PreparedShot, speed: SimScalar) {
        let target_pos = self
            .entities
            .get(&shot.target)
            .map_or(shot.origin, |t| t.position);
        let id = self.next_projectile_id;
        self.next_projectile_id = self.next_projectile_id.wrapping_add(1);

        let attacker_radius = self
            .catalog
            .get(
                self.entities
                    .get(&shot.attacker)
                    .map_or(UnitTypeId(0), |a| a.unit_type_id),
            )
            .map_or(SimScalar::HALF, |a| a.collision_radius);
        let (target_radius, target_mover) = self
            .entities
            .get(&shot.target)
            .and_then(|t| self.catalog.get(t.unit_type_id).ok())
            .map_or((SimScalar::HALF, MoverType::Ground), |a| {
                (a.collision_radius, a.mover_type)
            });

        let delta = target_pos - shot.origin;
        let dist = delta.length();
        let dir = delta.normalize_or_zero();

        let eff_dist =
            (dist - target_radius - attacker_radius).max(SimScalar::from_num(MIN_FLIGHT_DISTANCE));
        let ticks_to_impact = (eff_dist / speed)
            .saturating_ceil()
            .saturating_to_u32()
            .max(1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "presentation only: this value drives no simulation decision"
        )]
        let flight_duration_secs =
            (ticks_to_impact as f32) / f32::from(sim_types::TICKS_PER_SECOND);

        // Emit visual projectile event for client VFX rendering. The flight time above uses the
        // range edges (a structure's engine radius); the visual's ends lie on the drawn bubbles,
        // so a projectile lands on a dome's glass (plan `2026-11-18-structure-radius` D-6).
        let (attacker_drawn, target_drawn) = self.drawn_radii(&shot);
        let spawn_pos_2d = shot.origin + dir * attacker_drawn;
        let impact_pos_2d = target_pos - dir * target_drawn;
        let z_spawn = self.map.terrain_altitude_at(spawn_pos_2d);
        let z_impact = self.map.terrain_altitude_at(impact_pos_2d);

        let vx = (impact_pos_2d.x - spawn_pos_2d.x).to_f32() / flight_duration_secs;
        let vy = (impact_pos_2d.y - spawn_pos_2d.y).to_f32() / flight_duration_secs;
        let vz = (z_impact - z_spawn) / flight_duration_secs;

        self.pending_events
            .push(CombatEventVisual::ProjectileVisual {
                pos: [
                    spawn_pos_2d.x.to_f32(),
                    spawn_pos_2d.y.to_f32(),
                    z_spawn + 0.5,
                ],
                vel: [vx, vy, vz],
                owner: shot.owner,
                duration_secs: Some(flight_duration_secs),
                source: shot.source,
                ends: Some(StrikeEnds {
                    attacker: shot.attacker,
                    target: shot.target,
                }),
            });
        self.presentation.record_projectile(id, shot.source);

        let remaining_lifetime_ticks = shot.weapon.max_projectile_lifetime_ticks;
        let target_teleport_count = self
            .entities
            .get(&shot.target)
            .map_or(0, |target| target.teleport_count);
        let (cause, aimed_at) = (AttackCause::launch(&shot), shot.target);
        self.projectiles.push(ActiveProjectile {
            id,
            owner: shot.owner,
            attacker_id: Some(shot.attacker),
            source_pos: shot.origin,
            current_pos: shot.origin,
            target_id: Some(shot.target),
            target_pos,
            target_mover,
            speed,
            weapon_spec: shot.weapon,
            source_slug: Some(shot.source_slug),
            remaining_lifetime_ticks,
            strikes: shot.strikes,
            target_teleport_count,
        });
        self.record_attack_presentation(cause, aimed_at);
    }

    /// Resolves a hitscan, beam or melee shot: direct strikes, then splash, then visuals.
    ///
    /// A shot whose area is centred on the attacker ([`SplashImpact::Attacker`]) makes no direct
    /// strike: its area is searched around the attacker's release position, whether or not the
    /// target still stands, and strikes the target like any other unit in it; then, if the area
    /// says so, its other-level strike runs (see [`Self::strike_target_on_other_level`]).
    fn resolve_instant_shot(&mut self, shot: &PreparedShot) {
        let source = HitSource {
            weapon: &shot.weapon,
            slug: Some(&shot.source_slug),
            owner: shot.owner,
            attacker: Some(shot.attacker),
            origin: shot.origin,
            strikes: shot.strikes,
            effect: shot.source,
        };
        if let Some(splash) = shot
            .weapon
            .splash
            .as_ref()
            .filter(|splash| splash.impact == SplashImpact::Attacker)
        {
            let (area, aimed) = self.area_at_attacker(shot);
            self.apply_splash(source, splash, area, None);
            if splash.strike_target_on_other_level {
                let strike = AreaStrike {
                    plane: area.plane,
                    angle: area.angle,
                    primary: None,
                };
                self.strike_target_on_other_level(source, splash, strike, shot.target);
            }
            if let Some(aimed) = aimed {
                self.emit_instant_hit_visual(shot, aimed);
            }
            return;
        }
        let Some(hit) = self.apply_weapon_hit(source, shot.target) else {
            return;
        };
        if let Some(splash) = &shot.weapon.splash {
            self.apply_splash(source, splash, hit, Some(shot.target));
        }
        self.emit_instant_hit_visual(shot, hit);
    }

    /// Where the area of an attacker-centred `shot` lands, and where its delivery look aims.
    ///
    /// The area is searched around the attacker's release position (`shot.origin`), on the
    /// plane of the target (the attacker's when the target is gone), its hits arriving from the
    /// attacker's side (from its heading when the target is gone). Its radius is the target's
    /// collision radius, zero when the target is gone, which only a target radius condition
    /// reads (an attacker-centred area is never offset). The second value is the target's
    /// position for the shot's delivery event, `None` when the target is gone.
    fn area_at_attacker(&self, shot: &PreparedShot) -> (ResolvedHit, Option<ResolvedHit>) {
        let attacker = self.entities.get(&shot.attacker);
        let target = self.entities.get(&shot.target).and_then(|target| {
            self.catalog
                .get(target.unit_type_id)
                .ok()
                .map(|arch| (target.position, arch))
        });
        let plane = target.map_or_else(
            || {
                attacker
                    .and_then(|attacker| self.catalog.get(attacker.unit_type_id).ok())
                    .map_or(MoverType::Ground, |arch| arch.mover_type)
            },
            |(_, arch)| arch.mover_type,
        );
        let angle = target.map_or_else(
            || attacker.map_or(SimAngle::ZERO, |attacker| attacker.heading),
            |(position, _)| hit_angle(shot.origin, position),
        );
        let area = ResolvedHit {
            position: shot.origin,
            angle,
            plane,
            radius: target.map_or(SimScalar::ZERO, |(_, arch)| arch.collision_radius),
        };
        let aimed = target.map(|(position, arch)| ResolvedHit {
            position,
            angle,
            plane,
            radius: arch.collision_radius,
        });
        (area, aimed)
    }

    /// Applies the strikes `source` carries to `target_id`, then the target's retaliation.
    ///
    /// Returns where the shot landed, or `None` when the target no longer exists. A target
    /// that fails the weapon filter when the hit lands ([`Self::is_weapon_target_available`]:
    /// typically killed earlier this phase, or, for a missile that does not validate the
    /// weapon in flight, cloaked undetected or unseen at the landing) takes no strikes, but the
    /// landed hit still carries any area damage.
    fn apply_weapon_hit(
        &mut self,
        source: HitSource<'_>,
        target_id: EntityId,
    ) -> Option<ResolvedHit> {
        let available = self.is_weapon_target_available(source.owner, target_id);
        let target = self.entities.get_mut(&target_id)?;
        let target_arch = self.catalog.get(target.unit_type_id).ok()?;
        let hit = ResolvedHit {
            position: target.position,
            angle: hit_angle(source.origin, target.position),
            plane: target_arch.mover_type,
            radius: target_arch.collision_radius,
        };
        if !available {
            return Some(hit);
        }
        let attacker_upgrades = Self::upgrades_of(&self.player_upgrades, source.owner);
        let target_upgrades = Self::upgrades_of(&self.player_upgrades, target.owner);
        let raw = raw_strike_damage(source.weapon, source.slug, attacker_upgrades, target_arch);
        for _ in 0..source.strikes {
            strike_through_barrier(
                target,
                target_arch,
                target_upgrades,
                source.weapon,
                raw,
                hit.angle,
                &mut self.presentation,
            );
        }
        self.record_attack_presentation(AttackCause::strike(source), target_id);
        if let Some(attacker) = source.attacker {
            self.apply_combat_retaliation_prevalidated(target_id, attacker);
        }
        self.run_weapon_hit_effects(source, target_id, hit.position);
        Some(hit)
    }

    /// Records what `cause`, landing on or aimed at `victim_id`, shows of its caster to the
    /// victim's owner: the reveal of a `Visible` cause ([`Self::record_attack_reveal`]) and the
    /// snapshot of a `Snapshot` cause ([`Self::record_attack_snapshot`]). The one call every
    /// cause makes (a weapon's strike from both strike paths, a weapon missile's release, a
    /// spell's damage and its unit-aimed missile), so they cannot drift apart (plans
    /// `2026-11-24-attacker-snapshot` D-2, `2026-11-27-launch-and-spell-visibility` D-2). A
    /// victim that no longer exists records nothing.
    pub(crate) fn record_attack_presentation(&mut self, cause: AttackCause, victim_id: EntityId) {
        self.record_attack_reveal(cause, victim_id);
        self.record_attack_snapshot(cause, victim_id);
    }

    /// Records, for the owner p of `victim_id`, a snapshot of the caster of `cause` in its pose
    /// now, replacing any earlier one of the same caster (plan `2026-11-24-attacker-snapshot`
    /// D-2, D-3). It is recorded only when all of these hold: the cause's visibility is
    /// `Snapshot`; p is not the caster's player; the caster exists, lives and is not hidden;
    /// p's interface does not show it ([`Self::shown_to`]); and, if it is concealed, p detects
    /// its cell, so the image never shows a unit p could not see in plain sight. As for the
    /// reveal, a strike reduced to nothing still counts. The record goes to p and p's allies; the
    /// simulation has no teams, so that is p alone (owner answer Q0101 4(b)). The record is
    /// presentation: no tick decision reads it.
    fn record_attack_snapshot(&mut self, cause: AttackCause, victim_id: EntityId) {
        if cause.visibility != AttackVisibility::Snapshot {
            return;
        }
        let Some(viewer) = self
            .entities
            .get(&victim_id)
            .map(|victim| victim.owner)
            .filter(|&owner| owner != cause.owner)
        else {
            return;
        };
        let Some(attacker) = cause
            .attacker
            .and_then(|id| self.entities.get(&id))
            .filter(|attacker| attacker.is_alive() && !attacker.is_hidden())
        else {
            return;
        };
        if Self::shown_to(&self.map, &self.vision_maps, viewer, attacker) {
            return;
        }
        let detected = || {
            self.vision_maps.get(viewer).is_some_and(|vision| {
                self.map
                    .world_to_cell(attacker.position)
                    .is_ok_and(|(cx, cy)| vision.is_detected(cx, cy))
            })
        };
        if attacker.is_concealed() && !detected() {
            return;
        }
        let Ok(arch) = self.catalog.get(attacker.unit_type_id) else {
            return;
        };
        let record = AttackSnapshotRecord {
            viewer,
            attacker: attacker.id,
            struck: Tick(self.current_tick),
            pose: snapshot_pose(attacker, arch),
        };
        self.presentation.record_attack_snapshot(record);
    }

    /// Records that `cause` landed on, or was aimed at, `victim_id`: a cause whose visibility is
    /// `Visible` reveals its caster to the victim's owner for its reveal ticks, raising the
    /// caster's counter for that player to them and never lowering it (`StarCraft` II's
    /// `AttackReveal`, plan `2026-11-23-vision-and-detection` D-5). A strike reduced to nothing
    /// by a barrier, shields or armour still counts, as the damage effect ran. Nothing happens
    /// for another visibility, a victim of the caster's own player, or a caster that is gone
    /// or dead. The reveal goes to the struck unit's owner and that player's allies (owner
    /// answer Q0101 4(b)); the simulation has no teams (`match_state.rs`), so a player's only
    /// ally is itself (plan `2026-11-24-owner-answers-cliff-and-vision` D-6).
    fn record_attack_reveal(&mut self, cause: AttackCause, victim_id: EntityId) {
        let AttackVisibility::Visible { reveal_ticks } = cause.visibility else {
            return;
        };
        let Some(viewer) = self
            .entities
            .get(&victim_id)
            .map(|victim| victim.owner)
            .filter(|&owner| owner != cause.owner)
        else {
            return;
        };
        let Some(attacker) = cause
            .attacker
            .and_then(|id| self.entities.get_mut(&id))
            .filter(|attacker| attacker.is_alive())
        else {
            return;
        };
        if let Some(ticks) = attacker.attack_reveal_ticks.get_mut(viewer) {
            *ticks = (*ticks).max(reveal_ticks);
        }
    }

    /// Runs on `target_id` every weapon hit effect an upgrade the attacker's owner completed
    /// unlocks for the attacker's archetype ([`sim_types::UpgradeEffect::WeaponHitEffect`]),
    /// in upgrade order, as `StarCraft` II runs a weapon's impact set behind a
    /// `CValidatorPlayerRequirement` (plan `2026-11-05-sc2-deviation-fixes` Amendment 26 cites
    /// the shipped case). The effect's own validators decide whether it takes; a dead target
    /// passes none.
    ///
    /// Only direct hits run them: splash strikes do not, as the game's impact set runs on the
    /// missile's target alone.
    fn run_weapon_hit_effects(&mut self, source: HitSource<'_>, target_id: EntityId, at: SimVec2) {
        let Some(slug) = source.slug else {
            return;
        };
        let count = self.upgrade_catalog.weapon_hit_effects(slug).len();
        for index in 0..count {
            // PERFORMANCE RATIONALE:
            // Why: the walk below needs `&mut self`, so the unlock cannot stay borrowed from
            // the upgrade catalog across it, and a hit must not allocate to copy it out.
            // What: the unlock is looked up again by index and only its resolved id is kept.
            // Trade-off: one more map lookup per unlock (one today) instead of a clone per hit.
            let Some(unlock) = self.upgrade_catalog.weapon_hit_effects(slug).get(index) else {
                break;
            };
            if !Self::upgrades_of(&self.player_upgrades, source.owner)
                .completed_upgrade_slugs
                .contains(&unlock.upgrade_slug)
            {
                continue;
            }
            let Some(root) = self.catalog.spells().effect_id(&unlock.effect) else {
                let diagnostic = SimulationDiagnostic::UnknownWeaponHitEffect {
                    tick: Tick(self.current_tick),
                    upgrade: unlock.upgrade_slug.clone(),
                    effect: unlock.effect.clone(),
                };
                self.tick_diagnostics.push(diagnostic);
                continue;
            };
            let context = EffectContext {
                owner: source.owner,
                caster: source.attacker,
                // A weapon hit opens no area or tether, the only readers of the ability index,
                // so the attacker's type is recorded with index 0.
                ability: AbilitySource {
                    unit_type: source.effect.unit_type,
                    ability: 0,
                },
                origin: source.origin,
                source_point: at,
                target: EffectTarget::Unit(target_id),
            };
            self.run_effect_walk(root, context);
        }
    }

    /// Applies `splash` around its search points (the impact point for a circle, each point of
    /// a line laid out from the attack) on the plane of the primary target.
    ///
    /// The impact point is the target's centre, or, for a splash with
    /// [`SplashSpec::offset_by_unit_radius`], the target's edge facing the attacker. A line
    /// whose searches are spaced in time ([`sim_types::SplashLine::period_ticks`], B-85) lands
    /// its first search now and leaves the rest to the attacker's [`Entity::pending_line`],
    /// replacing any line still pending; every other area lands whole now. See
    /// [`Self::strike_area`] for who is struck.
    ///
    /// An area with [`SplashSpec::target_radius_below`] does not open at all, searching nothing
    /// and publishing nothing, when `hit.radius` (the struck target's collision radius, zero for
    /// a point) is that limit or more (B-84).
    fn apply_splash(
        &mut self,
        source: HitSource<'_>,
        splash: &SplashSpec,
        hit: ResolvedHit,
        primary: Option<EntityId>,
    ) {
        if splash
            .target_radius_below
            .is_some_and(|limit| hit.radius >= limit)
        {
            return;
        }
        let mut points = std::mem::take(&mut self.combat.splash_points);
        let direction = self.attack_direction(source, hit.position);
        let impact = if splash.offset_by_unit_radius {
            hit.position - direction * hit.radius
        } else {
            hit.position
        };
        splash.search_points(source.origin, impact, direction, &mut points);
        let area = AreaStrike {
            plane: hit.plane,
            angle: hit.angle,
            primary,
        };
        let line = splash_line_view(splash, source.origin, impact, direction);
        let period = match &splash.shape {
            SplashShape::Line(line) => line.period_ticks,
            SplashShape::Circle => 0,
        };
        let count = u8::try_from(points.len()).unwrap_or(u8::MAX);
        match (source.attacker, primary) {
            (Some(attacker), Some(target)) if period > 0 && points.len() > 1 => {
                let mut struck = Vec::new();
                self.emit_search_visuals(source, splash, hit.plane, line, &points[..1], (0, count));
                self.strike_area(source, splash, area, &points[..1], Some(&mut struck));
                if let Some(entity) = self.entities.get_mut(&attacker) {
                    entity.pending_line = Some(PendingLine {
                        points: points[1..].to_vec(),
                        ticks_until_next: period,
                        plane: hit.plane,
                        angle: hit.angle,
                        primary: target,
                        struck,
                        view: PendingLineView {
                            effect: source.effect,
                            line,
                            count,
                        },
                    });
                }
            }
            _ => {
                self.emit_search_visuals(source, splash, hit.plane, line, &points, (0, count));
                self.strike_area(source, splash, area, &points, None);
            }
        }
        self.combat.splash_points = points;
    }

    /// Strikes every unit `splash` reaches from the search centres `points` on `area`'s plane.
    ///
    /// A zone reaches a unit whose edge lies within it: the distance is measured from the search
    /// point to the unit's centre, less the unit's collision radius, and never below zero (owner
    /// answer to Q0063). A unit inside several searches is struck once, at the fraction of its
    /// nearest search point, and a unit in `struck` not at all (the game's `noMarkers` validator
    /// on the damage effect); each unit struck is appended to `struck`. An area kept to its level
    /// ([`SplashCliff::Same`]) counts only the search points whose cliff span overlaps the
    /// unit's: a ramp cell spans both its landings, and an area on the air plane or a unit in the
    /// air spans every level (plan `2026-11-23-splash-cliff-levels` D-1; owner answer Q0099
    /// 1(c), 2(b), plan `2026-11-24-owner-answers-cliff-and-vision` D-2). Spares `area.primary`, already struck directly,
    /// and the attacker, which never damages itself. Each struck entity then runs its
    /// retaliation against the attacker.
    fn strike_area(
        &mut self,
        source: HitSource<'_>,
        splash: &SplashSpec,
        area: AreaStrike,
        points: &[SimVec2],
        struck: Option<&mut Vec<EntityId>>,
    ) {
        let mut splash_hits = std::mem::take(&mut self.combat.splash_hits);
        splash_hits.clear();
        let mut levels = std::mem::take(&mut self.combat.splash_levels);
        levels.clear();
        let same_level = splash.cliff == SplashCliff::Same;
        if same_level {
            levels.extend(
                points
                    .iter()
                    .map(|&point| level_span(&self.map, point, area.plane)),
            );
        }
        let reach = splash.reach();
        for (&other_id, other) in &self.entities {
            if struck
                .as_deref()
                .is_some_and(|struck| struck.contains(&other_id))
                || !Self::area_may_strike(source, splash, area, other_id, other)
            {
                continue;
            }
            let Ok(other_arch) = self.catalog.get(other.unit_type_id) else {
                continue;
            };
            let edge_distance = |point: &SimVec2| {
                (point.distance(other.position) - other_arch.collision_radius).max(SimScalar::ZERO)
            };
            let Some(mut distance) = points
                .iter()
                .map(edge_distance)
                .min()
                .filter(|&distance| distance <= reach)
            else {
                continue;
            };
            if same_level {
                // Only a unit already in reach reads its level, so the map is read once per
                // unit the area could strike, not once per entity.
                let own = level_span(&self.map, other.position, other_arch.mover_type);
                let Some(on_level) = points
                    .iter()
                    .zip(&levels)
                    .filter(|&(_, &level)| level.overlaps(own))
                    .map(|(point, _)| edge_distance(point))
                    .min()
                    .filter(|&distance| distance <= reach)
                else {
                    continue;
                };
                distance = on_level;
            }
            if let Some(raw) = self.area_raw_damage(source, splash, area, other_arch, distance) {
                splash_hits.push((other_id, raw));
            }
        }
        self.combat.splash_levels = levels;
        self.land_area_hits(source, area, &splash_hits, struck);
        self.combat.splash_hits = splash_hits;
    }

    /// The other-level strike of an attacker-centred area (plan `2026-11-23-splash-cliff-levels`
    /// D-3, `StarCraft` II's `BanelingVolatileBurstDirectFallbackSet` on `CliffLevelNotEqual`):
    /// when `target_id`'s cliff span does not overlap that of the attacker's release position
    /// (`source.origin`) on the area's plane, the exact complement of the area's level test (a
    /// ramp spans both its landings and the air every level, plan
    /// `2026-11-24-owner-answers-cliff-and-vision` D-2), the target takes
    /// `splash`'s strike directly, at the fraction of the
    /// first zone (distance zero), once, and runs its retaliation. It passes the area's own
    /// filters, as `strike_area` applies them, without their distance and level tests. It runs
    /// whether or not the area opened, and does nothing when the target is gone or on the
    /// attacker's level, where the area alone strikes it.
    fn strike_target_on_other_level(
        &mut self,
        source: HitSource<'_>,
        splash: &SplashSpec,
        area: AreaStrike,
        target_id: EntityId,
    ) {
        let Some(target) = self.entities.get(&target_id) else {
            return;
        };
        let Ok(target_arch) = self.catalog.get(target.unit_type_id) else {
            return;
        };
        if level_span(&self.map, target.position, target_arch.mover_type).overlaps(level_span(
            &self.map,
            source.origin,
            area.plane,
        )) || !Self::area_may_strike(source, splash, area, target_id, target)
        {
            return;
        }
        let Some(raw) = self.area_raw_damage(source, splash, area, target_arch, SimScalar::ZERO)
        else {
            return;
        };
        let mut splash_hits = std::mem::take(&mut self.combat.splash_hits);
        splash_hits.clear();
        splash_hits.push((target_id, raw));
        self.land_area_hits(source, area, &splash_hits, None);
        self.combat.splash_hits = splash_hits;
    }

    /// Whether an area of `source` may strike `other` (id `other_id`) at all, before its reach:
    /// not the primary target `area` spares, not the attacker, damageable, and an enemy unless
    /// the area has friendly fire.
    fn area_may_strike(
        source: HitSource<'_>,
        splash: &SplashSpec,
        area: AreaStrike,
        other_id: EntityId,
        other: &Entity,
    ) -> bool {
        Some(other_id) != area.primary
            && Some(other_id) != source.attacker
            && Self::is_damageable(other)
            && (splash.friendly_fire || other.owner != source.owner)
    }

    /// The raw damage, before barrier and armour, an area of `source` deals a unit of
    /// `other_arch` whose edge lies `distance` world units from the search point, or `None`
    /// when the unit is off the area's plane, outside the weapon's target flags, or beyond the
    /// last zone.
    fn area_raw_damage(
        &self,
        source: HitSource<'_>,
        splash: &SplashSpec,
        area: AreaStrike,
        other_arch: &UnitArchetype,
        distance: SimScalar,
    ) -> Option<SimScalar> {
        if !shares_combat_plane(area.plane, other_arch.mover_type)
            || !source.weapon.target_flags.intersects(other_arch.attributes)
        {
            return None;
        }
        splash_raw_damage(
            source.weapon,
            source.slug,
            Self::upgrades_of(&self.player_upgrades, source.owner),
            splash,
            distance,
            other_arch,
        )
    }

    /// Strikes each entity of `hits` with its raw damage through its barrier and armour, from
    /// `area`'s direction, appends it to `struck` and runs its retaliation against the attacker.
    fn land_area_hits(
        &mut self,
        source: HitSource<'_>,
        area: AreaStrike,
        hits: &[(EntityId, SimScalar)],
        mut struck: Option<&mut Vec<EntityId>>,
    ) {
        for &(id, raw) in hits {
            if let Some(entity) = self.entities.get_mut(&id) {
                if let Ok(arch) = self.catalog.get(entity.unit_type_id) {
                    let upgrades = Self::upgrades_of(&self.player_upgrades, entity.owner);
                    strike_through_barrier(
                        entity,
                        arch,
                        upgrades,
                        source.weapon,
                        raw,
                        area.angle,
                        &mut self.presentation,
                    );
                }
            }
            if let Some(struck) = struck.as_deref_mut() {
                struck.push(id);
            }
            self.record_attack_presentation(AttackCause::strike(source), id);
            if let Some(attacker) = source.attacker {
                self.apply_combat_retaliation_prevalidated(id, attacker);
            }
        }
    }

    /// Records one renderer event per search of `splash` at `points`, on `plane`, for an attack
    /// by `source`. `line` is the whole line the searches lie on, if the splash is a line (see
    /// [`splash_line_view`]); the searches are numbered from `first` of `count`, so a line whose
    /// searches land on several ticks (B-85) numbers each by its place in the line.
    fn emit_search_visuals(
        &mut self,
        source: HitSource<'_>,
        splash: &SplashSpec,
        plane: MoverType,
        line: Option<(SimVec2, SimVec2, SimScalar)>,
        points: &[SimVec2],
        (first, count): (u8, u8),
    ) {
        let lift = if plane == MoverType::Air {
            crate::world::AIR_UNIT_ALTITUDE
        } else {
            0.0
        };
        let at = |point: SimVec2| {
            [
                point.x.to_f32(),
                point.y.to_f32(),
                self.map.terrain_altitude_at(point) + lift,
            ]
        };
        let zones = splash_zone_radii(splash);
        let line = line.map(|(from, to, spacing)| (at(from), at(to), spacing.to_f32()));
        for (index, &point) in (first..).zip(points) {
            self.pending_events.push(CombatEventVisual::SplashVisual {
                center: at(point),
                zones,
                line: line.map(|(from, to, spacing)| LineView {
                    from,
                    to,
                    spacing,
                    index,
                    count,
                }),
                source: source.effect,
                owner: source.owner,
            });
        }
    }

    /// The unit vector of an attack from `source.origin` towards `impact`. When the two
    /// coincide, the attacker's facing (the game's `OffsetFacingFallback` `SourceUnit`, `core`
    /// EffectData:39); zero when the attacker no longer exists either.
    fn attack_direction(&self, source: HitSource<'_>, impact: SimVec2) -> SimVec2 {
        (impact - source.origin).normalize().unwrap_or_else(|_| {
            source
                .attacker
                .and_then(|id| self.entities.get(&id))
                .map_or(SimVec2::ZERO, |attacker| {
                    SimVec2::new(attacker.heading.cos(), attacker.heading.sin())
                })
        })
    }

    /// Records the renderer event for an instant hit.
    fn emit_instant_hit_visual(&mut self, shot: &PreparedShot, hit: ResolvedHit) {
        let z = self.map.terrain_altitude_at(shot.origin);
        let tz = self.map.terrain_altitude_at(hit.position);
        let from = [shot.origin.x.to_f32(), shot.origin.y.to_f32(), z + 0.5];
        let to = [hit.position.x.to_f32(), hit.position.y.to_f32(), tz + 0.5];
        let owner = shot.owner;
        let source = shot.source;
        let ends = Some(StrikeEnds {
            attacker: shot.attacker,
            target: shot.target,
        });
        let event = match shot.weapon.visual_kind {
            WeaponVisualKind::Hitscan => CombatEventVisual::HitscanTracer {
                from,
                to,
                owner,
                source,
                ends,
            },
            WeaponVisualKind::LaserBeam => CombatEventVisual::BeamVisual {
                from,
                to,
                is_lightning: false,
                owner,
                source,
                ends,
            },
            WeaponVisualKind::LightningArc => CombatEventVisual::BeamVisual {
                from,
                to,
                is_lightning: true,
                owner,
                source,
                ends,
            },
            WeaponVisualKind::MeleeSlash => CombatEventVisual::MeleeSlashVisual {
                pos: to,
                angle_rad: hit.angle.to_radians_f32(),
                radius: 0.5,
                owner,
                source,
                ends,
            },
            _ => return,
        };
        self.pending_events.push(event);
    }

    /// Advances every projectile one tick, then resolves the ones that finished.
    fn advance_projectiles(&mut self) {
        let mut finished = std::mem::take(&mut self.combat.finished_projectiles);
        finished.clear();
        let mut projectiles = std::mem::take(&mut self.projectiles);
        for (index, projectile) in projectiles.iter_mut().enumerate() {
            match self.advance_projectile(projectile) {
                ProjectileStep::Flying => {}
                step => finished.push((index, step)),
            }
        }
        self.projectiles = projectiles;

        for &(index, step) in finished.iter().rev() {
            let projectile = self.projectiles.remove(index);
            let source = self
                .presentation
                .take_projectile_source(projectile.id)
                .unwrap_or_else(|| self.unrecorded_projectile_source(&projectile));
            if matches!(step, ProjectileStep::Impact) {
                self.resolve_projectile_impact(&projectile, source);
            }
        }
        self.combat.finished_projectiles = finished;
    }

    /// Moves one projectile, handling lifetime, target loss and surface impact.
    fn advance_projectile(&mut self, proj: &mut ActiveProjectile) -> ProjectileStep {
        proj.remaining_lifetime_ticks = proj.remaining_lifetime_ticks.saturating_sub(1);
        if proj.remaining_lifetime_ticks == 0 {
            // Fueled out / timed out before reaching target
            return ProjectileStep::Disjointed;
        }

        let Some(target_radius) = self.track_projectile_target(proj) else {
            return ProjectileStep::Disjointed;
        };

        let delta = proj.target_pos - proj.current_pos;
        if delta.length() <= proj.speed + target_radius {
            ProjectileStep::Impact
        } else {
            proj.current_pos += delta.normalize_or_zero() * proj.speed;
            ProjectileStep::Flying
        }
    }

    /// Refreshes a projectile's aim point and returns the surface radius to stop at.
    ///
    /// The target is followed while it passes the weapon's filters (5.0.14's `ValidateWeapon`;
    /// [`Self::passes_weapon_filters`], vision included, so a target that walks into fog is lost)
    /// or, for a weapon that clears `missile_validation.weapon` (the Brood Lord's missiles),
    /// while it is alive and not loaded: such a missile follows it through fog, cloak and
    /// invulnerability and leaves the filters to the impact ([`Self::apply_weapon_hit`]). It is
    /// lost otherwise and, if the weapon validates teleports (`missile_validation.teleport`,
    /// `ValidateTeleport`), when its teleport count moved on since the launch. The teleport is
    /// checked before the aim point is refreshed, so a lost missile keeps the point where the
    /// target stood before it (plan `2026-11-24-weapon-validation` D-4).
    ///
    /// Returns `None` when the projectile disjoints because its target was lost.
    fn track_projectile_target(&mut self, proj: &mut ActiveProjectile) -> Option<SimScalar> {
        let Some(target_id) = proj.target_id else {
            return Some(SimScalar::ZERO);
        };
        let validation = proj.weapon_spec.missile_validation;
        let teleported = validation.teleport
            && self
                .entities
                .get(&target_id)
                .is_some_and(|target| target.teleport_count != proj.target_teleport_count);
        let followed = if validation.weapon {
            self.is_weapon_target_available(proj.owner, target_id)
        } else {
            // [INF] A loaded target is not followed: its position is its transport's (plan
            // `2026-11-24-weapon-validation` D-4, R-6).
            self.entities
                .get(&target_id)
                .is_some_and(|target| target.is_alive() && !target.is_loaded_in_transport())
        };
        if followed && !teleported {
            let mut radius = SimScalar::HALF;
            if let Some(target) = self.entities.get(&target_id) {
                proj.target_pos = target.position;
                radius = self
                    .catalog
                    .get(target.unit_type_id)
                    .map_or(SimScalar::HALF, |a| a.collision_radius);
            }
            return Some(radius);
        }
        // Target became unavailable (hidden in cargo or a structure, cloaked without detector,
        // invulnerable, died, or expired) or teleported.
        match proj.weapon_spec.target_loss_behavior {
            ProjectileTargetLossBehavior::Fizzle => None,
            ProjectileTargetLossBehavior::ImpactLastKnownPosition => {
                // Keep target_pos as a ballistic coordinate.
                proj.target_id = None;
                Some(SimScalar::ZERO)
            }
            ProjectileTargetLossBehavior::RetargetNearest { search_radius } => {
                let (new_id, new_pos, new_radius, new_mover) =
                    self.nearest_projectile_retarget(proj, search_radius)?;
                proj.target_id = Some(new_id);
                proj.target_pos = new_pos;
                proj.target_mover = new_mover;
                proj.target_teleport_count = self
                    .entities
                    .get(&new_id)
                    .map_or(0, |target| target.teleport_count);
                Some(new_radius)
            }
        }
    }

    /// Finds the nearest available enemy a seeking projectile may lock onto.
    fn nearest_projectile_retarget(
        &mut self,
        proj: &ActiveProjectile,
        search_radius: SimScalar,
    ) -> Option<(EntityId, SimVec2, SimScalar, MoverType)> {
        let mut best: Option<(EntityId, SimVec2, SimScalar, MoverType, SimScalar)> = None;
        let search_r2 = search_radius * search_radius;

        self.scratch_neighbors.clear();
        self.spatial_hash.query_radius(
            proj.current_pos,
            search_radius,
            &mut self.scratch_neighbors,
        );
        for &candidate_id in &self.scratch_neighbors {
            if Some(candidate_id) == proj.target_id || Some(candidate_id) == proj.attacker_id {
                continue;
            }
            let Some(candidate) = self.entities.get(&candidate_id) else {
                continue;
            };
            if candidate.owner == proj.owner
                || !Self::passes_weapon_filters(candidate, &self.map, &self.vision_maps, proj.owner)
            {
                continue;
            }
            let Ok(candidate_arch) = self.catalog.get(candidate.unit_type_id) else {
                continue;
            };
            if !proj
                .weapon_spec
                .target_flags
                .intersects(candidate_arch.attributes)
            {
                continue;
            }
            let d2 = proj.current_pos.distance_sq(candidate.position);
            if d2 <= search_r2 && best.as_ref().is_none_or(|b| d2 < b.4) {
                best = Some((
                    candidate_id,
                    candidate.position,
                    candidate_arch.collision_radius,
                    candidate_arch.mover_type,
                    d2,
                ));
            }
        }
        best.map(|(id, position, radius, mover, _)| (id, position, radius, mover))
    }

    /// The source of a projectile launched outside combat resolution, which recorded none: the
    /// archetype its slug names, with the default weapon key.
    fn unrecorded_projectile_source(&self, proj: &ActiveProjectile) -> EffectSource {
        EffectSource {
            unit_type: proj
                .source_slug
                .as_deref()
                .and_then(|slug| self.catalog.id_by_slug(slug))
                .unwrap_or_default(),
            ..EffectSource::default()
        }
    }

    /// Applies the damage of a landed projectile, fired by `effect`: on its target, or on bare
    /// ground.
    fn resolve_projectile_impact(&mut self, proj: &ActiveProjectile, effect: EffectSource) {
        let source = HitSource {
            weapon: &proj.weapon_spec,
            slug: proj.source_slug.as_deref(),
            owner: proj.owner,
            attacker: proj.attacker_id,
            origin: proj.source_pos,
            strikes: proj.strikes,
            effect,
        };
        if let Some(target_id) = proj.target_id {
            let Some(hit) = self.apply_weapon_hit(source, target_id) else {
                return;
            };
            if let Some(splash) = &proj.weapon_spec.splash {
                self.apply_splash(source, splash, hit, Some(target_id));
            }
        } else if let Some(splash) = &proj.weapon_spec.splash {
            // Ballistic impact at ground coordinate (ImpactLastKnownPosition) with splash
            let hit = ResolvedHit {
                position: proj.target_pos,
                angle: hit_angle(proj.source_pos, proj.target_pos),
                plane: proj.target_mover,
                radius: SimScalar::ZERO,
            };
            self.apply_splash(source, splash, hit, None);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::catalog::EffectNode;
    use crate::effects::ActiveMissile;
    use crate::entity::ActiveProjectile;
    use crate::test_support::world;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{
        AbilitySource, EffectAnchor, EffectContext, EffectId, EffectTarget, MoverType, PlayerId,
        SimScalar, SimVec2,
    };

    /// T-M4 (plan `2026-11-11-contact-and-missiles` D-6): a weapon projectile and an effect
    /// missile with the same speed, start and target, the target receding 0.25 a tick, impact
    /// on the same step. Each step moves the target first, so both paths see the same aim.
    #[test]
    fn a_projectile_and_an_effect_missile_impact_on_the_same_step() {
        let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        let mut world = world(map, 5);
        let spells = world.catalog.spells_handle();
        let (node, speed) = (0..spells.node_count())
            .find_map(|index| {
                let id = EffectId(u16::try_from(index).ok()?);
                match spells.node(id)?.kind {
                    EffectNode::LaunchMissile {
                        to: EffectAnchor::TargetUnit,
                        speed,
                        ..
                    } => Some((id, speed)),
                    _ => None,
                }
            })
            .expect("the shipped catalog launches a homing missile");
        let weapon = world
            .catalog
            .archetypes()
            .find_map(|arch| arch.weapon.clone())
            .expect("the shipped catalog has a weapon");
        let kind = world
            .catalog
            .archetypes()
            .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a mobile ground unit");
        let owner = PlayerId(0);
        let start = SimVec2::from_i32(10, 20);
        let target = world
            .spawn_unit(kind, owner, SimVec2::from_i32(16, 20))
            .expect("invariant: a free spot on an empty map");
        world.projectiles.push(ActiveProjectile {
            id: 0,
            owner,
            attacker_id: None,
            source_pos: start,
            current_pos: start,
            target_id: Some(target),
            target_pos: SimVec2::from_i32(16, 20),
            target_mover: MoverType::Ground,
            speed,
            weapon_spec: weapon,
            source_slug: None,
            remaining_lifetime_ticks: u16::MAX,
            strikes: 1,
            target_teleport_count: 0,
        });
        let unit = EffectTarget::Unit(target);
        world.effects.missiles.push(ActiveMissile {
            id: 0,
            node,
            context: EffectContext {
                owner,
                caster: None,
                ability: AbilitySource::default(),
                origin: start,
                source_point: start,
                target: unit,
            },
            position: start,
            target: unit,
        });
        let receding = SimScalar::from_num(0.25);
        let (mut projectile_step, mut missile_step) = (None, None);
        for step in 1..=40_u32 {
            world.entities.get_mut(&target).expect("alive").position.x += receding;
            // The projectile follows only a target its owner sees (plan
            // `2026-11-24-weapon-validation` D-3): the target's own sight shows its cell.
            world.update_vision();
            if projectile_step.is_none() {
                world.advance_projectiles();
                if world.projectiles.is_empty() {
                    projectile_step = Some(step);
                }
            }
            if missile_step.is_none() {
                world.step_effect_missiles();
                if world.effects.missiles.is_empty() {
                    missile_step = Some(step);
                }
            }
        }
        assert!(projectile_step.is_some(), "the projectile impacts");
        assert_eq!(projectile_step, missile_step);
    }

    /// R-14 (plan `2026-11-18-structure-radius` D-6): a projectile fired at a 3×3 dome whose
    /// radius is 1.8125 flies for the ticks its radius gives (`⌈max(d − R − r, 0.05) ÷ speed⌉`),
    /// but its visual arrives on the drawn dome: `pos + vel · duration` lies half the footprint
    /// (1.5) from the dome's centre, towards the shooter.
    #[test]
    fn a_projectile_visual_to_a_dome_arrives_on_its_drawn_dome() {
        use sim_types::{CombatEventVisual, ShapeKind};
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        let mut world = world(map, 0x0B_135);
        let shooter_arch = world
            .catalog
            .archetypes()
            .find(|arch| {
                matches!(arch.shape, ShapeKind::UnitSphere)
                    && arch.mover_type == MoverType::Ground
                    && arch
                        .weapon_against(MoverType::Ground, None)
                        .is_some_and(|weapon| weapon.projectile_speed.is_some())
            })
            .cloned()
            .expect("the shipped catalog has a ground unit firing projectiles at ground");
        let weapon = shooter_arch
            .weapon_against(MoverType::Ground, None)
            .cloned()
            .expect("checked above");
        let speed = weapon.projectile_speed.expect("checked above");
        let dome = world
            .catalog
            .archetypes()
            .find(|arch| {
                matches!(
                    arch.shape,
                    ShapeKind::BuildingDome {
                        footprint_w: 3,
                        footprint_h: 3,
                        ..
                    }
                )
            })
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a 3x3 structure");
        let radius = SimScalar::from_num(1.8125);
        world
            .catalog
            .get_mut(dome)
            .expect("the dome's archetype")
            .collision_radius = radius;
        // Cells 20 to 23: the centre is (21.5, 21.5).
        let structure = world
            .spawn_structure(dome, PlayerId(1), 20, 20)
            .expect("open ground");
        let origin = SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(21.5));
        let shooter = world
            .spawn_unit(shooter_arch.unit_type_id, PlayerId(0), origin)
            .expect("open ground");
        let source = super::weapon_source(&shooter_arch, &world.entities[&shooter], &weapon);
        world.pending_events.clear();
        world.launch_projectile(
            super::PreparedShot {
                attacker: shooter,
                target: structure,
                weapon,
                source_slug: shooter_arch.slug.clone(),
                origin,
                owner: PlayerId(0),
                strikes: 1,
                source,
            },
            speed,
        );
        let (pos, vel, duration) = world
            .pending_events
            .iter()
            .find_map(|event| match event {
                CombatEventVisual::ProjectileVisual {
                    pos,
                    vel,
                    duration_secs: Some(duration),
                    ..
                } => Some((*pos, *vel, *duration)),
                _ => None,
            })
            .expect("the launch publishes its visual");
        let arrival = (pos[0] + vel[0] * duration, pos[1] + vel[1] * duration);
        assert!((arrival.0 - 20.0).abs() < 1e-4, "{arrival:?}");
        assert!((arrival.1 - 21.5).abs() < 1e-4, "{arrival:?}");
        let gap = (SimScalar::from_num(9.5) - radius - shooter_arch.collision_radius)
            .max(SimScalar::from_num(super::MIN_FLIGHT_DISTANCE));
        let ticks = (gap / speed).saturating_ceil().saturating_to_u32().max(1);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a flight of a few dozen ticks, recovered from its published seconds"
        )]
        let published = (duration * f32::from(sim_types::TICKS_PER_SECOND)).round() as u32;
        assert_eq!(published, ticks, "the flight time follows the radius");
    }

    /// Plan `2026-11-23-splash-centres` D-1: an attacker-centred area does not depend on its
    /// target. A shot whose target no longer exists when it is dispatched (no entity under its
    /// id) still lands its area around the attacker, striking an enemy beside it; the same shot
    /// centred on the target lands nothing.
    #[test]
    fn an_attacker_centred_area_lands_without_its_target() {
        use sim_types::{AttributeFlags, EntityId, SplashImpact, SplashShape};
        let strike = |impact: SplashImpact| {
            let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
                .expect("invariant: a flat map");
            let mut world = world(map, 0x5_C3E7);
            let arch = world
                .catalog
                .archetypes()
                .find(|arch| {
                    arch.mover_type == MoverType::Ground
                        && arch.max_speed > SimScalar::ZERO
                        && arch.collision_radius <= SimScalar::ONE
                })
                .cloned()
                .expect("the shipped catalog has a mobile ground unit");
            let mut weapon = world
                .catalog
                .archetypes()
                .flat_map(|arch| [&arch.weapon, &arch.ground_weapon, &arch.air_weapon])
                .flatten()
                .find(|weapon| {
                    weapon.splash.as_ref().is_some_and(|splash| {
                        splash.shape == SplashShape::Circle && splash.reach() >= SimScalar::ONE
                    })
                })
                .cloned()
                .expect("the shipped catalog has a circular splash of reach 1 or more");
            weapon.target_flags = AttributeFlags::all();
            weapon.projectile_speed = None;
            weapon.suicide_on_attack = false;
            weapon.structure_strike = None;
            let splash = weapon.splash.as_mut().expect("chosen for its splash");
            splash.impact = impact;
            splash.offset_by_unit_radius = false;
            splash.friendly_fire = false;
            let origin = SimVec2::from_i32(10, 10);
            let attacker = world
                .spawn_unit(arch.unit_type_id, PlayerId(0), origin)
                .expect("open ground");
            // Edge to edge 0.1 from the attacker, so its edge lies within a reach of 1.
            let beside = origin
                + SimVec2::new(
                    arch.collision_radius + arch.collision_radius + SimScalar::from_num(0.1),
                    SimScalar::ZERO,
                );
            let bystander = world
                .spawn_unit(arch.unit_type_id, PlayerId(1), beside)
                .expect("open ground");
            let missing = EntityId(u32::MAX);
            assert!(!world.entities.contains_key(&missing));
            let full = world.entities[&bystander].hp + world.entities[&bystander].shields;
            let source = super::weapon_source(&arch, &world.entities[&attacker], &weapon);
            world.dispatch_shot(super::PreparedShot {
                attacker,
                target: missing,
                weapon,
                source_slug: arch.slug.clone(),
                origin,
                owner: PlayerId(0),
                strikes: 1,
                source,
            });
            full - (world.entities[&bystander].hp + world.entities[&bystander].shields)
        };
        assert!(
            strike(SplashImpact::Attacker) > SimScalar::ZERO,
            "the area landed"
        );
        assert_eq!(strike(SplashImpact::Target), SimScalar::ZERO, "the control");
    }

    /// Plan `2026-11-23-splash-cliff-levels` D-3: the other-level strike passes the area's own
    /// filters. A burst from the plateau at a target below it strikes an enemy the weapon may
    /// strike (the control), but not a unit of the attacker's own player (friendly fire off) nor
    /// one whose attributes the weapon's target flags exclude. No order reaches the first (an
    /// attack order on one's own unit is an attack-move), so the shot is dispatched directly.
    #[test]
    fn the_other_level_strike_keeps_the_areas_filters() {
        use sim_types::{AttributeFlags, SplashCliff, SplashImpact, SplashShape};
        let strike = |target_owner: PlayerId, flags: AttributeFlags| {
            let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
                .expect("invariant: a valid size");
            for y in 0..32 {
                for x in 16..32 {
                    map.cell_mut(x, y)
                        .expect("invariant: inside the map")
                        .cliff_level = CliffLevel(1);
                }
            }
            let mut world = world(map, 0x5_C11F);
            let arch = world
                .catalog
                .archetypes()
                .find(|arch| {
                    arch.mover_type == MoverType::Ground
                        && arch.max_speed > SimScalar::ZERO
                        && arch.collision_radius <= SimScalar::HALF
                })
                .cloned()
                .expect("the shipped catalog has a small mobile ground unit");
            let mut weapon = world
                .catalog
                .archetypes()
                .flat_map(|arch| [&arch.weapon, &arch.ground_weapon, &arch.air_weapon])
                .flatten()
                .find(|weapon| {
                    weapon.splash.as_ref().is_some_and(|splash| {
                        splash.shape == SplashShape::Circle && splash.reach() >= SimScalar::ONE
                    })
                })
                .cloned()
                .expect("the shipped catalog has a circular splash of reach 1 or more");
            weapon.target_flags = flags;
            weapon.projectile_speed = None;
            weapon.suicide_on_attack = false;
            weapon.structure_strike = None;
            let splash = weapon.splash.as_mut().expect("chosen for its splash");
            splash.impact = SplashImpact::Attacker;
            splash.offset_by_unit_radius = false;
            splash.friendly_fire = false;
            splash.cliff = SplashCliff::Same;
            splash.strike_target_on_other_level = true;
            let origin = SimVec2::new(SimScalar::from_num(16.6), SimScalar::from_num(10));
            let attacker = world
                .spawn_unit(arch.unit_type_id, PlayerId(0), origin)
                .expect("open ground");
            let below = SimVec2::new(SimScalar::from_num(15.4), SimScalar::from_num(10));
            let target = world
                .spawn_unit(arch.unit_type_id, target_owner, below)
                .expect("open ground");
            let full = world.entities[&target].hp + world.entities[&target].shields;
            let source = super::weapon_source(&arch, &world.entities[&attacker], &weapon);
            world.dispatch_shot(super::PreparedShot {
                attacker,
                target,
                weapon,
                source_slug: arch.slug.clone(),
                origin,
                owner: PlayerId(0),
                strikes: 1,
                source,
            });
            full - (world.entities[&target].hp + world.entities[&target].shields)
        };
        assert!(
            strike(PlayerId(1), AttributeFlags::all()) > SimScalar::ZERO,
            "the control: an enemy across the cliff is struck"
        );
        assert_eq!(
            strike(PlayerId(0), AttributeFlags::all()),
            SimScalar::ZERO,
            "the attacker's own unit"
        );
        assert_eq!(
            strike(PlayerId(1), AttributeFlags::empty()),
            SimScalar::ZERO,
            "an enemy the weapon's target flags exclude"
        );
    }

    /// A world on a 32x32 map whose cells with x >= 16 stand one cliff level up (no ramp): a
    /// ranged ground attacker of player 0 on the plateau's edge, a victim of player 1 below it,
    /// the weapon the attacker strikes with, and the vision of this tick computed.
    fn snapshot_bench() -> (
        crate::SimWorld,
        sim_types::EntityId,
        sim_types::EntityId,
        crate::catalog::WeaponSpec,
    ) {
        use sim_types::{AttackVisibility, ShapeKind};
        let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        for y in 0..32 {
            for x in 16..32 {
                map.cell_mut(x, y)
                    .expect("invariant: on the map")
                    .cliff_level = CliffLevel(1);
            }
        }
        let mut world = world(map, 0x183);
        let (kind, weapon) = world
            .catalog
            .archetypes()
            .filter(|arch| {
                matches!(arch.shape, ShapeKind::UnitSphere)
                    && arch.mover_type == MoverType::Ground
                    && !arch.is_detector()
            })
            .find_map(|arch| {
                arch.weapon_against(MoverType::Ground, None)
                    .filter(|w| {
                        w.attack_visibility == AttackVisibility::Snapshot
                            && w.max_range >= SimScalar::from_i32(4)
                    })
                    .map(|w| (arch.unit_type_id, w.clone()))
            })
            .expect("the shipped catalog has a ranged ground unit with a Snapshot weapon");
        let attacker = world
            .spawn_unit(
                kind,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(16.5)),
            )
            .expect("invariant: a free spot");
        let victim = world
            .spawn_unit(
                kind,
                PlayerId(1),
                SimVec2::new(SimScalar::from_num(13.5), SimScalar::from_num(16.5)),
            )
            .expect("invariant: a free spot");
        world.update_vision();
        (world, attacker, victim, weapon)
    }

    /// Lands one strike of `weapon` by `attacker` on `victim` through the presentation hook and
    /// returns how many snapshots player 1 keeps.
    fn strike(
        world: &mut crate::SimWorld,
        attacker: sim_types::EntityId,
        victim: sim_types::EntityId,
        weapon: &crate::catalog::WeaponSpec,
    ) -> usize {
        let owner = world.entities[&attacker].owner;
        let source = super::HitSource {
            weapon,
            slug: None,
            owner,
            attacker: Some(attacker),
            origin: world.entities[&attacker].position,
            strikes: 1,
            effect: sim_types::EffectSource::default(),
        };
        world.record_attack_presentation(super::AttackCause::strike(source), victim);
        world.presentation.attack_snapshots_of(PlayerId(1)).count()
    }

    /// Plan `2026-11-24-attacker-snapshot` D-2: with every condition of the record rule met, a
    /// strike records one snapshot; each condition failing alone records none.
    #[test]
    fn each_condition_of_the_snapshot_rule_decides_alone() {
        use sim_types::AttackVisibility;
        let (mut world, attacker, victim, weapon) = snapshot_bench();
        assert_eq!(strike(&mut world, attacker, victim, &weapon), 1, "all met");

        for visibility in [
            AttackVisibility::Visible { reveal_ticks: 54 },
            AttackVisibility::Hidden,
        ] {
            let (mut world, attacker, victim, mut weapon) = snapshot_bench();
            weapon.attack_visibility = visibility;
            assert_eq!(
                strike(&mut world, attacker, victim, &weapon),
                0,
                "{visibility:?}"
            );
        }

        let (mut world, attacker, victim, weapon) = snapshot_bench();
        world.entities.get_mut(&victim).expect("spawned").owner = PlayerId(0);
        assert_eq!(
            strike(&mut world, attacker, victim, &weapon),
            0,
            "own player"
        );
        assert_eq!(
            world.presentation.attack_snapshots_of(PlayerId(0)).count(),
            0
        );

        let (mut world, attacker, victim, weapon) = snapshot_bench();
        world.entities.get_mut(&attacker).expect("spawned").hp = SimScalar::ZERO;
        assert_eq!(
            strike(&mut world, attacker, victim, &weapon),
            0,
            "dead attacker"
        );

        let (mut world, attacker, victim, weapon) = snapshot_bench();
        world
            .entities
            .get_mut(&attacker)
            .expect("spawned")
            .transport_carrier = Some(victim);
        assert_eq!(
            strike(&mut world, attacker, victim, &weapon),
            0,
            "hidden attacker"
        );

        let (mut world, attacker, victim, weapon) = snapshot_bench();
        let kind = world.entities[&victim].unit_type_id;
        world
            .spawn_unit(
                kind,
                PlayerId(1),
                SimVec2::new(SimScalar::from_num(19.5), SimScalar::from_num(16.5)),
            )
            .expect("invariant: a free spot");
        world.update_vision();
        assert_eq!(
            strike(&mut world, attacker, victim, &weapon),
            0,
            "seen attacker"
        );

        let (mut world, attacker, victim, weapon) = snapshot_bench();
        world
            .entities
            .get_mut(&attacker)
            .expect("spawned")
            .is_cloaked = true;
        assert_eq!(
            strike(&mut world, attacker, victim, &weapon),
            0,
            "undetected"
        );
    }

    /// Plan `2026-11-24-attacker-snapshot` D-3: a concealed attacker whose cell the struck
    /// player detects, though it does not see it, leaves a snapshot.
    #[test]
    fn a_detected_concealed_attacker_leaves_a_snapshot() {
        use sim_types::ShapeKind;
        let (mut world, attacker, victim, weapon) = snapshot_bench();
        world
            .entities
            .get_mut(&attacker)
            .expect("spawned")
            .is_cloaked = true;
        let detector = world
            .catalog
            .archetypes()
            .find(|arch| arch.is_detector() && matches!(arch.shape, ShapeKind::BuildingDome { .. }))
            .map(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a detector structure");
        let structure = world
            .spawn_structure(detector, PlayerId(1), 12, 19)
            .expect("invariant: a free site below the plateau");
        world.update_vision();
        let at = world.entities[&attacker].position;
        let (cx, cy) = world.map.world_to_cell(at).expect("invariant: on the map");
        let vision = world.vision_maps.get(PlayerId(1)).expect("a slot");
        assert!(vision.is_detected(cx, cy), "premise: detected");
        assert_ne!(
            vision.visibility_at(cx, cy),
            sim_types::VisibilityState::Visible,
            "premise: unseen"
        );
        assert!(world.entities.contains_key(&structure));
        assert_eq!(strike(&mut world, attacker, victim, &weapon), 1);
    }

    /// The conditions of the record rule (plan `2026-11-24-attacker-snapshot` D-2, D-3; plan
    /// `2026-11-23-vision-and-detection` D-5), each set alone on [`snapshot_bench`].
    #[derive(Clone, Copy, Debug)]
    enum Condition {
        AllMet,
        OwnPlayer,
        DeadAttacker,
        HiddenAttacker,
        SeenAttacker,
        UndetectedConcealed,
        DetectedConcealed,
    }

    impl Condition {
        const ALL: [Self; 7] = [
            Self::AllMet,
            Self::OwnPlayer,
            Self::DeadAttacker,
            Self::HiddenAttacker,
            Self::SeenAttacker,
            Self::UndetectedConcealed,
            Self::DetectedConcealed,
        ];

        /// [`snapshot_bench`] with this condition set.
        fn bench(
            self,
        ) -> (
            crate::SimWorld,
            sim_types::EntityId,
            sim_types::EntityId,
            crate::catalog::WeaponSpec,
        ) {
            use sim_types::ShapeKind;
            let (mut world, attacker, victim, weapon) = snapshot_bench();
            match self {
                Self::AllMet => {}
                Self::OwnPlayer => {
                    world.entities.get_mut(&victim).expect("spawned").owner = PlayerId(0);
                }
                Self::DeadAttacker => {
                    world.entities.get_mut(&attacker).expect("spawned").hp = SimScalar::ZERO;
                }
                Self::HiddenAttacker => {
                    world
                        .entities
                        .get_mut(&attacker)
                        .expect("spawned")
                        .transport_carrier = Some(victim);
                }
                Self::SeenAttacker => {
                    let kind = world.entities[&victim].unit_type_id;
                    world
                        .spawn_unit(
                            kind,
                            PlayerId(1),
                            SimVec2::new(SimScalar::from_num(19.5), SimScalar::from_num(16.5)),
                        )
                        .expect("invariant: a free spot");
                    world.update_vision();
                }
                Self::UndetectedConcealed | Self::DetectedConcealed => {
                    world
                        .entities
                        .get_mut(&attacker)
                        .expect("spawned")
                        .is_cloaked = true;
                    if matches!(self, Self::DetectedConcealed) {
                        let detector = world
                            .catalog
                            .archetypes()
                            .find(|arch| {
                                arch.is_detector()
                                    && matches!(arch.shape, ShapeKind::BuildingDome { .. })
                            })
                            .map(|arch| arch.unit_type_id)
                            .expect("the shipped catalog has a detector structure");
                        world
                            .spawn_structure(detector, PlayerId(1), 12, 19)
                            .expect("invariant: a free site below the plateau");
                        world.update_vision();
                    }
                }
            }
            (world, attacker, victim, weapon)
        }
    }

    /// What one record call left: the images each of players 0 and 1 keeps, and the attacker's
    /// reveal counter for each.
    type Recorded = ([usize; 2], [u16; 2]);

    fn recorded(world: &crate::SimWorld, attacker: sim_types::EntityId) -> Recorded {
        let players = [PlayerId(0), PlayerId(1)];
        let images = players.map(|p| world.presentation.attack_snapshots_of(p).count());
        let reveals = players.map(|p| {
            world.entities[&attacker]
                .attack_reveal_ticks
                .get(p)
                .copied()
                .unwrap_or(0)
        });
        (images, reveals)
    }

    /// What the record rule leaves for `visibility` under `condition`, written from the rule:
    /// a snapshot where every condition of the image holds (a concealed attacker only where
    /// detected), a reveal where the victim is another player's and the attacker lives.
    fn expected(visibility: sim_types::AttackVisibility, condition: Condition) -> Recorded {
        use sim_types::AttackVisibility;
        match (visibility, condition) {
            (AttackVisibility::Snapshot, Condition::AllMet | Condition::DetectedConcealed) => {
                ([0, 1], [0, 0])
            }
            (
                AttackVisibility::Visible { reveal_ticks },
                Condition::AllMet
                | Condition::HiddenAttacker
                | Condition::SeenAttacker
                | Condition::UndetectedConcealed
                | Condition::DetectedConcealed,
            ) => ([0, 0], [0, reveal_ticks]),
            _ => ([0, 0], [0, 0]),
        }
    }

    /// The three causes of plan `2026-11-27-launch-and-spell-visibility` D-2.
    #[derive(Clone, Copy, Debug)]
    enum Cause {
        Strike,
        Launch,
        Spell,
    }

    /// V-21 (plan `2026-11-27-launch-and-spell-visibility` D-2): for each visibility and each
    /// condition of the record rule, a weapon strike records exactly what the rule says, as the
    /// weapon-hit path recorded before the cause replaced it (the table passed on that path,
    /// WP1's tree); a missile's release with that launch visibility, and a spell with that
    /// node visibility cast by the same unit at the same victim, record the same.
    #[test]
    fn every_cause_records_by_the_rule_for_each_visibility_and_condition() {
        use sim_types::{AbilitySource, AttackVisibility, EffectContext, EffectTarget};
        for visibility in [
            AttackVisibility::Snapshot,
            AttackVisibility::Visible { reveal_ticks: 54 },
            AttackVisibility::Hidden,
        ] {
            for condition in Condition::ALL {
                for cause in [Cause::Strike, Cause::Launch, Cause::Spell] {
                    let (mut world, attacker, victim, mut weapon) = condition.bench();
                    let owner = world.entities[&attacker].owner;
                    let origin = world.entities[&attacker].position;
                    let recorded_cause = match cause {
                        Cause::Strike => {
                            weapon.attack_visibility = visibility;
                            super::AttackCause::strike(super::HitSource {
                                weapon: &weapon,
                                slug: None,
                                owner,
                                attacker: Some(attacker),
                                origin,
                                strikes: 1,
                                effect: sim_types::EffectSource::default(),
                            })
                        }
                        Cause::Launch => {
                            weapon.launch_visibility = visibility;
                            super::AttackCause::launch(&super::PreparedShot {
                                attacker,
                                target: victim,
                                weapon: weapon.clone(),
                                source_slug: String::new(),
                                origin,
                                owner,
                                strikes: 1,
                                source: sim_types::EffectSource::default(),
                            })
                        }
                        Cause::Spell => super::AttackCause::spell(
                            visibility,
                            &EffectContext {
                                owner,
                                caster: Some(attacker),
                                ability: AbilitySource::default(),
                                origin,
                                source_point: origin,
                                target: EffectTarget::Unit(victim),
                            },
                        ),
                    };
                    world.record_attack_presentation(recorded_cause, victim);
                    assert_eq!(
                        recorded(&world, attacker),
                        expected(visibility, condition),
                        "{visibility:?} {condition:?} {cause:?}"
                    );
                }
            }
        }
    }

    /// A flat 32 × 32 world with an armed ground unit of player 0 at (10, 10), one of player 1
    /// at (12, 10) and a second of player 1 at (14, 14), the carrier; vision updated.
    fn lost_swing_bench() -> (
        crate::SimWorld,
        sim_types::EntityId,
        sim_types::EntityId,
        sim_types::EntityId,
    ) {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: a flat map");
        let mut world = world(map, 0x0207);
        let kind = world
            .catalog
            .archetypes()
            .filter(|arch| {
                arch.mover_type == MoverType::Ground
                    && arch.max_speed > SimScalar::ZERO
                    && arch.weapon_against(MoverType::Ground, None).is_some()
            })
            .min_by_key(|arch| arch.unit_type_id)
            .expect("the shipped catalog has a mobile armed ground unit")
            .unit_type_id;
        let at = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
        let attacker = world
            .spawn_unit(kind, PlayerId(0), at(10, 10))
            .expect("invariant: a free spot");
        let target = world
            .spawn_unit(kind, PlayerId(1), at(12, 10))
            .expect("invariant: a free spot");
        let carrier = world
            .spawn_unit(kind, PlayerId(1), at(14, 14))
            .expect("invariant: a free spot");
        world.update_vision();
        (world, attacker, target, carrier)
    }

    /// Whether `attacker`'s swing on `target` lives in `world` now, by the shared rule.
    fn live(
        world: &crate::SimWorld,
        attacker: sim_types::EntityId,
        target: sim_types::EntityId,
    ) -> bool {
        let context = super::StrikeContext {
            catalog: &world.catalog,
            upgrades: &world.player_upgrades,
            map: &world.map,
            vision_maps: &world.vision_maps,
        };
        world.entities.get(&target).is_some_and(|target| {
            crate::SimWorld::swing_target_live(
                &context,
                &world.entities[&attacker],
                crate::SimWorld::target_snapshot(&world.catalog, target),
            )
        })
    }

    /// Plan `2026-11-25-wind-up-and-hold-facing` D-1: a swing's target is live while it is seen
    /// and damageable, and each loss alone ends it: death, boarding, cloak without detection,
    /// and leaving its player's sight.
    #[test]
    fn each_loss_alone_ends_a_swing() {
        let (world, attacker, target, _) = lost_swing_bench();
        assert!(live(&world, attacker, target), "seen and damageable");

        let (mut dead, attacker, target, _) = lost_swing_bench();
        dead.entities.get_mut(&target).expect("target").hp = SimScalar::ZERO;
        assert!(!live(&dead, attacker, target), "dead");

        let (mut boarded, attacker, target, carrier) = lost_swing_bench();
        boarded
            .try_load_unit_into_transport(target, carrier)
            .expect("invariant: the carrier takes its own player's unit");
        assert!(!live(&boarded, attacker, target), "boarded");

        let (mut cloaked, attacker, target, _) = lost_swing_bench();
        cloaked
            .entities
            .get_mut(&target)
            .expect("target")
            .is_cloaked = true;
        assert!(!live(&cloaked, attacker, target), "cloaked, undetected");

        let (mut unseen, attacker, target, _) = lost_swing_bench();
        unseen.entities.get_mut(&target).expect("target").position =
            SimVec2::new(SimScalar::from_i32(30), SimScalar::from_i32(30));
        unseen.update_vision();
        assert!(!live(&unseen, attacker, target), "out of sight");
    }

    /// Plan `2026-11-25-wind-up-and-hold-facing` D-1: the pass ends a wind-up on a gone target,
    /// keeps one on a live target, and leaves a unit whose weapon the weapon phase does not step
    /// (a boarded attacker) as it is.
    #[test]
    fn the_pass_ends_only_lost_swings_the_weapon_phase_steps() {
        use crate::entity::WeaponPhase;
        let windup = |target_id| WeaponPhase::Windup {
            target_id,
            remaining_ticks: 3,
        };
        let (mut world, attacker, target, carrier) = lost_swing_bench();
        world
            .entities
            .get_mut(&attacker)
            .expect("attacker")
            .weapon_phase = windup(target);
        world.end_lost_swings();
        assert_eq!(
            world.entities[&attacker].weapon_phase,
            windup(target),
            "live"
        );

        let gone = sim_types::EntityId(u32::MAX);
        world
            .entities
            .get_mut(&attacker)
            .expect("attacker")
            .weapon_phase = windup(gone);
        world.end_lost_swings();
        assert_eq!(
            world.entities[&attacker].weapon_phase,
            WeaponPhase::Ready,
            "gone"
        );

        world
            .entities
            .get_mut(&carrier)
            .expect("carrier")
            .weapon_phase = windup(gone);
        world
            .try_load_unit_into_transport(carrier, target)
            .expect("invariant: the target carries its own player's unit");
        world.end_lost_swings();
        assert_eq!(
            world.entities[&carrier].weapon_phase,
            windup(gone),
            "a carried unit's weapon is frozen"
        );
    }
}
