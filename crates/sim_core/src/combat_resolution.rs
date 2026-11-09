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

use crate::catalog::UnitCatalog;
use crate::catalog::{SplashSpec, UnitArchetype, WeaponSpec};
use crate::combat::{
    charge_random_delay, mitigate_weapon_strike, raw_strike_damage, shares_combat_plane,
    splash_raw_damage, step_entity_weapon, WeaponTarget,
};
use crate::entity::{ActiveProjectile, Entity, PendingStrikes, WeaponPhase};
use crate::weapon_timing::{resolve_attack_period, AttackRate, SimulationDiagnostic};
use crate::world::SimWorld;
use sim_map::{PlayerVisionMap, SimMap};
use sim_types::{
    CombatEventVisual, EntityId, MoverType, PerPlayer, PlayerId, PlayerUpgradeState,
    ProjectileTargetLossBehavior, SimAngle, SimScalar, SimVec2, Tick, UnitOrder, UnitTypeId,
    WeaponVisualKind,
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
    /// together, one on a release that spaces them and on each later strike.
    strikes: u8,
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
}

/// Where a direct hit landed, for the area damage and visuals that follow it.
#[derive(Clone, Copy)]
struct ResolvedHit {
    position: SimVec2,
    angle: SimAngle,
    /// Combat plane of the area damage this hit may cause.
    plane: MoverType,
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
    /// Whether `viewer`'s weapons may strike the entity `snapshot` describes: it passes the
    /// damage gate and, if cloaked, is revealed to `viewer`.
    fn is_available(&self, viewer: PlayerId, snapshot: TargetSnapshot) -> bool {
        snapshot.damageable
            && (!snapshot.cloaked
                || SimWorld::is_cloak_revealed(
                    self.map,
                    self.vision_maps,
                    viewer,
                    snapshot.position,
                ))
    }
}

/// Direction a hit arrives from, as stored for hull-impact rendering.
fn hit_angle(origin: SimVec2, target: SimVec2) -> SimAngle {
    SimAngle::atan2(origin.y - target.y, origin.x - target.x)
}

/// Lands one strike of `weapon` carrying `raw` damage (before armour) on `target`.
///
/// The target's barrier, if its archetype has one, absorbs first (B-41). What it lets through
/// is mitigated by shields and armour and applied. A strike the barrier absorbs whole deals
/// nothing: no 0.5 minimum, no hit flash, and the regeneration delays keep running.
fn strike_through_barrier(
    target: &mut Entity,
    target_arch: &UnitArchetype,
    target_upgrades: &PlayerUpgradeState,
    weapon: &WeaponSpec,
    raw: SimScalar,
    angle: SimAngle,
) {
    let raw = target
        .barrier
        .absorb(target_arch.capabilities.barrier.as_ref(), raw);
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
        self.advance_weapon_cycles();

        let mut shots = std::mem::take(&mut self.combat.shots);
        for shot in shots.drain(..) {
            self.dispatch_shot(shot);
        }
        self.combat.shots = shots;

        self.advance_projectiles();
    }

    /// Records every entity's targeting data so weapons can read targets while mutating
    /// their owners.
    fn capture_target_snapshots(&mut self) {
        let snapshots = &mut self.combat.snapshots;
        snapshots.clear();
        for (&id, entity) in &self.entities {
            let (radius, mover) = self
                .catalog
                .get(entity.unit_type_id)
                .map_or((SimScalar::HALF, MoverType::Ground), |a| {
                    (a.collision_radius, a.mover_type)
                });
            // Retain authored weapon-selection metadata, but never expose cargo
            // coordinates to aim/damage. Air-only windup timing must not silently
            // switch to the missing-target Ground fallback when a target boards.
            snapshots.push((
                id,
                TargetSnapshot {
                    position: entity.position,
                    radius,
                    mover,
                    damageable: Self::is_damageable(entity),
                    cloaked: entity.is_cloaked,
                },
            ));
        }
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
            if !entity.is_alive()
                || entity.channeling_remaining_ticks > 0
                || entity.is_loaded_in_transport()
                || !entity.is_completed
            {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };

            // A swing stays bound to the target it started on. Otherwise the orders decide:
            // an attack order names the target, and turrets and holders use `current_target`.
            let target_opt_id = if let WeaponPhase::Windup { target_id, .. } = entity.weapon_phase {
                Some(target_id)
            } else if let Some(UnitOrder::AttackTarget { target, .. }) = entity.order_queue.front()
            {
                Some(*target)
            } else {
                let has_turret = entity
                    .siege_weapon
                    .as_ref()
                    .or(arch.ground_weapon.as_ref())
                    .or(arch.weapon.as_ref())
                    .is_some_and(|w| w.has_turret);
                (has_turret || entity.is_on_hold())
                    .then_some(entity.current_target)
                    .flatten()
            };

            let target_snapshot = target_opt_id.and_then(|tid| combat.snapshot(tid));
            let target_mover =
                target_snapshot.map_or_else(|| Self::idle_weapon_plane(entity, arch), |t| t.mover);

            let Some(weapon) = Self::resolved_weapon(
                &self.player_upgrades,
                entity,
                arch,
                target_mover,
                attack_mult,
            )
            .map(ResolvedWeapon::to_spec) else {
                // A target that vanished mid-swing may leave no weapon to resolve here.
                if matches!(entity.weapon_phase, WeaponPhase::Windup { .. }) {
                    entity.weapon_phase = WeaponPhase::Ready;
                }
                continue;
            };

            let target_info = target_opt_id.and_then(|id| {
                let snapshot = target_snapshot?;
                context
                    .is_available(entity.owner, snapshot)
                    .then_some(WeaponTarget {
                        id,
                        position: snapshot.position,
                        combined_radius: snapshot.radius + arch.collision_radius,
                    })
            });

            if step_entity_weapon(entity, &weapon, target_info) {
                charge_random_delay(entity, &weapon, rng);
                if let Some(target) = target_info {
                    let strikes = Self::begin_strikes(entity, &weapon, target.id, target_mover);
                    combat.shots.push(PreparedShot {
                        attacker: *id,
                        target: target.id,
                        weapon,
                        source_slug: arch.slug.clone(),
                        origin: entity.position,
                        owner: entity.owner,
                        strikes,
                    });
                }
            }
        }
    }

    /// Starts the strikes of an attack `entity` releases with `weapon` at `target` on `plane`,
    /// returning how many land on this release tick.
    ///
    /// A weapon whose strikes land together (`strike_interval_ticks` 0, or a single strike)
    /// lands them all now. Otherwise the first lands now and the rest wait in
    /// [`Entity::pending_strikes`], one every interval (B-2). A burst still pending is replaced;
    /// catalog validation ends every burst within its attack period, so only a period shortened
    /// below the burst (by upgrades or buffs) can do that.
    fn begin_strikes(
        entity: &mut Entity,
        weapon: &WeaponSpec,
        target: EntityId,
        plane: MoverType,
    ) -> u8 {
        if weapon.strike_interval_ticks == 0 || weapon.attacks_count < 2 {
            entity.pending_strikes = None;
            return weapon.attacks_count;
        }
        entity.pending_strikes = Some(PendingStrikes {
            target_id: target,
            plane,
            remaining: weapon.attacks_count - 1,
            ticks_until_next: weapon.strike_interval_ticks,
        });
        1
    }

    /// Counts down the pending strikes of `entity` (`id`) and queues the one that falls due.
    ///
    /// The burst ends when the attacker dies or boards a transport, or when it no longer has the
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
        if !entity.is_alive() || entity.is_loaded_in_transport() {
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
        let Some(weapon) =
            Self::resolved_weapon(context.upgrades, entity, arch, pending.plane, attack_rate)
                .map(ResolvedWeapon::to_spec)
        else {
            return;
        };
        let remaining = pending.remaining.saturating_sub(1);
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
                strikes: 1,
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
        let max_range =
            Self::effective_range_from_upgrades(upgrades, spec, &arch.slug, entity.owner);
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

    /// Spawns the projectile for `shot` and emits its renderer event.
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

        // Emit visual projectile event for client VFX rendering
        let spawn_pos_2d = shot.origin + dir * attacker_radius;
        let impact_pos_2d = target_pos - dir * target_radius;
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
            });

        let remaining_lifetime_ticks = shot.weapon.max_projectile_lifetime_ticks;
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
        });
    }

    /// Resolves a hitscan, beam or melee shot: direct strikes, then splash, then visuals.
    fn resolve_instant_shot(&mut self, shot: &PreparedShot) {
        let source = HitSource {
            weapon: &shot.weapon,
            slug: Some(&shot.source_slug),
            owner: shot.owner,
            attacker: Some(shot.attacker),
            origin: shot.origin,
            strikes: shot.strikes,
        };
        let Some(hit) = self.apply_weapon_hit(source, shot.target) else {
            return;
        };
        if let Some(splash) = &shot.weapon.splash {
            self.apply_splash(source, splash, hit, Some(shot.target));
        }
        self.emit_instant_hit_visual(shot, hit);
    }

    /// Applies the strikes `source` carries to `target_id`, then the target's retaliation.
    ///
    /// Returns where the shot landed, or `None` when the target no longer exists. A target
    /// that became unavailable after the shot was prepared, typically killed earlier this
    /// phase, takes no strikes, but the landed hit still carries any area damage.
    fn apply_weapon_hit(
        &mut self,
        source: HitSource<'_>,
        target_id: EntityId,
    ) -> Option<ResolvedHit> {
        let available = self.is_entity_available_for_combat(source.owner, target_id);
        let target = self.entities.get_mut(&target_id)?;
        let target_arch = self.catalog.get(target.unit_type_id).ok()?;
        let hit = ResolvedHit {
            position: target.position,
            angle: hit_angle(source.origin, target.position),
            plane: target_arch.mover_type,
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
            );
        }
        if let Some(attacker) = source.attacker {
            self.apply_combat_retaliation_prevalidated(target_id, attacker);
        }
        Some(hit)
    }

    /// Applies `splash` around its search points (`hit.position` for a circle, each point of
    /// a line laid out from the attack) on the plane of the primary target.
    ///
    /// A unit inside several searches is struck once, at the fraction of its nearest search
    /// point (the game's `noMarkers` validator on the damage effect). Spares `primary`,
    /// already struck directly, and the attacker, which never damages itself. Each struck
    /// entity then runs its retaliation against the attacker.
    fn apply_splash(
        &mut self,
        source: HitSource<'_>,
        splash: &SplashSpec,
        hit: ResolvedHit,
        primary: Option<EntityId>,
    ) {
        let mut splash_hits = std::mem::take(&mut self.combat.splash_hits);
        splash_hits.clear();
        let mut points = std::mem::take(&mut self.combat.splash_points);
        splash.search_points(
            source.origin,
            hit.position,
            self.attack_direction(source, hit.position),
            &mut points,
        );
        let attacker_upgrades = Self::upgrades_of(&self.player_upgrades, source.owner);
        let reach = splash.reach();
        for (&other_id, other) in &self.entities {
            if Some(other_id) == primary
                || Some(other_id) == source.attacker
                || !Self::is_damageable(other)
                || (!splash.friendly_fire && other.owner == source.owner)
            {
                continue;
            }
            let Some(distance) = points
                .iter()
                .map(|point| point.distance(other.position))
                .min()
                .filter(|&distance| distance <= reach)
            else {
                continue;
            };
            let Ok(other_arch) = self.catalog.get(other.unit_type_id) else {
                continue;
            };
            if !shares_combat_plane(hit.plane, other_arch.mover_type)
                || !source.weapon.target_flags.intersects(other_arch.attributes)
            {
                continue;
            }
            if let Some(raw) = splash_raw_damage(
                source.weapon,
                source.slug,
                attacker_upgrades,
                splash,
                distance,
                other_arch,
            ) {
                splash_hits.push((other_id, raw));
            }
        }
        for &(id, raw) in &splash_hits {
            if let Some(entity) = self.entities.get_mut(&id) {
                if let Ok(arch) = self.catalog.get(entity.unit_type_id) {
                    let upgrades = Self::upgrades_of(&self.player_upgrades, entity.owner);
                    strike_through_barrier(entity, arch, upgrades, source.weapon, raw, hit.angle);
                }
            }
            if let Some(attacker) = source.attacker {
                self.apply_combat_retaliation_prevalidated(id, attacker);
            }
        }
        self.combat.splash_hits = splash_hits;
        self.combat.splash_points = points;
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
        let event = match shot.weapon.visual_kind {
            WeaponVisualKind::Hitscan => CombatEventVisual::HitscanTracer { from, to, owner },
            WeaponVisualKind::LaserBeam => CombatEventVisual::BeamVisual {
                from,
                to,
                is_lightning: false,
                owner,
            },
            WeaponVisualKind::LightningArc => CombatEventVisual::BeamVisual {
                from,
                to,
                is_lightning: true,
                owner,
            },
            WeaponVisualKind::MeleeSlash => CombatEventVisual::MeleeSlashVisual {
                pos: to,
                angle_rad: hit.angle.to_radians_f32(),
                radius: 0.5,
                owner,
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
            if matches!(step, ProjectileStep::Impact) {
                self.resolve_projectile_impact(&projectile);
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
    /// Returns `None` when the projectile disjoints because its target was lost.
    fn track_projectile_target(&mut self, proj: &mut ActiveProjectile) -> Option<SimScalar> {
        let Some(target_id) = proj.target_id else {
            return Some(SimScalar::ZERO);
        };
        if self.is_entity_available_for_combat(proj.owner, target_id) {
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
        // Target became unavailable (cargo loaded, cloaked without detector, invulnerable,
        // died, or expired).
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
                || !Self::is_target_available_for_combat(
                    &self.entities,
                    &self.map,
                    &self.vision_maps,
                    proj.owner,
                    candidate_id,
                )
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

    /// Applies the damage of a landed projectile: on its target, or on bare ground.
    fn resolve_projectile_impact(&mut self, proj: &ActiveProjectile) {
        let source = HitSource {
            weapon: &proj.weapon_spec,
            slug: proj.source_slug.as_deref(),
            owner: proj.owner,
            attacker: proj.attacker_id,
            origin: proj.source_pos,
            strikes: proj.strikes,
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
            };
            self.apply_splash(source, splash, hit, None);
        }
    }
}
