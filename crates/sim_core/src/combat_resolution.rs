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

use crate::catalog::{SplashSpec, WeaponSpec};
use crate::combat::{
    calculate_hit_damage_upgraded, calculate_splash_damage_upgraded, shares_combat_plane,
    step_entity_weapon, DamageResolution, WeaponTarget,
};
use crate::entity::{ActiveProjectile, WeaponPhase};
use crate::weapon_timing::{resolve_attack_period, SimulationDiagnostic};
use crate::world::SimWorld;
use sim_types::{
    CombatEventVisual, EntityId, MoverType, PlayerId, PlayerUpgradeState,
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
}

/// Provenance of a hit, borrowed from a prepared shot or a landed projectile.
#[derive(Clone, Copy)]
struct HitSource<'a> {
    weapon: &'a WeaponSpec,
    slug: Option<&'a str>,
    owner: PlayerId,
    attacker: Option<EntityId>,
    origin: SimVec2,
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
    splash_hits: Vec<(EntityId, DamageResolution)>,
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

/// Upgrade state of `owner`; out-of-range owners share the last slot, as everywhere else.
fn upgrades_of(upgrades: &[PlayerUpgradeState; 8], owner: PlayerId) -> &PlayerUpgradeState {
    &upgrades[(owner.0 as usize).min(7)]
}

/// Direction a hit arrives from, as stored for hull-impact rendering.
fn hit_angle(origin: SimVec2, target: SimVec2) -> SimAngle {
    SimAngle::atan2(origin.y - target.y, origin.x - target.x)
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
            if !entity.is_alive()
                || entity.channeling_remaining_ticks > 0
                || entity.is_loaded_in_transport()
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
            let target_mover = target_snapshot.map_or(MoverType::Ground, |t| t.mover);

            let Some(mut weapon) = arch
                .weapon_against(target_mover, entity.active_siege_weapon())
                .cloned()
            else {
                // A target that vanished mid-swing may leave no weapon to resolve here.
                if matches!(entity.weapon_phase, WeaponPhase::Windup { .. }) {
                    entity.weapon_phase = WeaponPhase::Ready;
                }
                continue;
            };

            let target_info = target_opt_id.and_then(|id| {
                let snapshot = target_snapshot?;
                let available = snapshot.damageable
                    && (!snapshot.cloaked
                        || Self::is_cloak_revealed(
                            &self.map,
                            &self.vision_maps,
                            entity.owner,
                            snapshot.position,
                        ));
                available.then_some(WeaponTarget {
                    id,
                    position: snapshot.position,
                    combined_radius: snapshot.radius + arch.collision_radius,
                })
            });

            weapon.max_range = Self::effective_range_from_upgrades(
                &self.player_upgrades,
                &weapon,
                &arch.slug,
                entity.owner,
            );
            let delta = upgrades_of(&self.player_upgrades, entity.owner)
                .unit_stat_modifiers
                .get(&arch.slug)
                .map_or(0, |modifiers| modifiers.attack_cooldown_delta);
            weapon.period_ticks = resolve_attack_period(weapon.period_ticks, delta, attack_mult);

            if step_entity_weapon(entity, &weapon, target_info) {
                if let Some(target) = target_info {
                    combat.shots.push(PreparedShot {
                        attacker: *id,
                        target: target.id,
                        weapon,
                        source_slug: arch.slug.clone(),
                        origin: entity.position,
                        owner: entity.owner,
                    });
                }
            }
        }
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
        let flight_duration_secs = (ticks_to_impact as f32) / 25.0;

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
        };
        let Some(hit) = self.apply_weapon_hit(source, shot.target) else {
            return;
        };
        if let Some(splash) = &shot.weapon.splash {
            self.apply_splash(source, splash, hit, Some(shot.target));
        }
        self.emit_instant_hit_visual(shot, hit);
    }

    /// Applies every strike of `source` to `target_id`, then the target's retaliation.
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
        let attacker_upgrades = upgrades_of(&self.player_upgrades, source.owner);
        let target_upgrades = upgrades_of(&self.player_upgrades, target.owner);
        for _ in 0..source.weapon.attacks_count {
            let resolution = calculate_hit_damage_upgraded(
                source.weapon,
                source.slug,
                attacker_upgrades,
                target_arch,
                target_upgrades,
                target.shields,
                target.hp,
            );
            target.apply_damage(resolution.shield_damage, resolution.life_damage, hit.angle);
        }
        if let Some(attacker) = source.attacker {
            self.apply_combat_retaliation_prevalidated(target_id, attacker);
        }
        Some(hit)
    }

    /// Applies `splash` around `hit.position` on the plane of the primary target.
    ///
    /// Spares `primary`, already struck directly, and the attacker, which never damages
    /// itself. Each struck entity then runs its retaliation against the attacker.
    fn apply_splash(
        &mut self,
        source: HitSource<'_>,
        splash: &SplashSpec,
        hit: ResolvedHit,
        primary: Option<EntityId>,
    ) {
        let mut splash_hits = std::mem::take(&mut self.combat.splash_hits);
        splash_hits.clear();
        let attacker_upgrades = upgrades_of(&self.player_upgrades, source.owner);
        for (&other_id, other) in &self.entities {
            if Some(other_id) == primary
                || Some(other_id) == source.attacker
                || !Self::is_damageable(other)
                || (!splash.friendly_fire && other.owner == source.owner)
            {
                continue;
            }
            let distance = hit.position.distance(other.position);
            if distance > splash.outer_radius {
                continue;
            }
            let Ok(other_arch) = self.catalog.get(other.unit_type_id) else {
                continue;
            };
            if !shares_combat_plane(hit.plane, other_arch.mover_type)
                || !source.weapon.target_flags.intersects(other_arch.attributes)
            {
                continue;
            }
            let resolution = calculate_splash_damage_upgraded(
                source.weapon,
                source.slug,
                attacker_upgrades,
                splash,
                distance,
                other_arch,
                upgrades_of(&self.player_upgrades, other.owner),
                other.shields,
                other.hp,
            );
            if let Some(resolution) = resolution {
                splash_hits.push((other_id, resolution));
            }
        }
        for &(id, resolution) in &splash_hits {
            if let Some(entity) = self.entities.get_mut(&id) {
                entity.apply_damage(resolution.shield_damage, resolution.life_damage, hit.angle);
            }
            if let Some(attacker) = source.attacker {
                self.apply_combat_retaliation_prevalidated(id, attacker);
            }
        }
        self.combat.splash_hits = splash_hits;
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
