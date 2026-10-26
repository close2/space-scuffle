//! # Authoritative Simulation World & 10-Phase Tick Pipeline
//!
//! Encapsulates all authoritative simulation components in [`SimWorld`], driving
//! the deterministic lockstep loop through an invariant 10-phase execution pipeline at 25 Hz.
//!
//! ## Invariants Maintained
//! 1. All calculations use deterministic fixed-point math (`SimScalar`, `SimVec2`, `SimAngle`).
//! 2. Entity iteration order is strictly sorted by `EntityId` to guarantee identical checksums.
//! 3. Simulation ticks run at an invariant rate of 25 Hz (40 ms per tick).
//! 4. Zero engine / GPU dependencies; compiles cleanly in headless CI.

use crate::catalog::{UnitArchetype, UnitCatalog, WeaponSpec};
use crate::checksum::compute_world_checksum;
use crate::combat::{
    calculate_hit_damage_upgraded, calculate_splash_damage_upgraded, compute_threat_priority,
    step_entity_weapon, TargetPriorityKey,
};
use crate::entity::{ActiveProjectile, Entity, WeaponPhase};
use crate::error::{SimCoreError, SimResult};
use crate::steering::{
    check_cluster_arrival, compute_push_displacement, compute_unified_local_steering,
    PushEntityParams, SpatialHashGrid, SteeringObstacle, SPATIAL_CELL_SIZE,
};
use crate::upgrades::UpgradeCatalog;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, PathRequestWithVision, Pathfinder, PlayerVisionMap,
    SimMap, VisionSource,
};
use sim_types::{
    AbilityEffectSpec, AnimState, AttributeFlags, BubbleRenderInstance, CombatEventVisual,
    EntityId, PlayerId, PlayerUpgradeState, ProjectileTargetLossBehavior, RenderSnapshot,
    ShapeKind, SimAngle, SimScalar, SimVec2, TeamId, Tick, TickBundle, UnitOrder, UnitTypeId,
    VisibilityState, WeaponVisualKind,
};
use std::collections::BTreeMap;

// PERFORMANCE RATIONALE:
// Why: Allocating intermediate vectors during the 10-phase tick loop causes allocator fragmentation
// and degrades execution speed from >10,000 ticks/sec down to <2,000 ticks/sec.
// What: Persistent scratch buffers (spatial query results, candidate targets, dead entity lists)
// are stored directly inside `SimWorld` and cleared each tick without heap reallocation.
// Trade-off: Slightly larger persistent struct footprint in exchange for zero per-tick heap allocations.

/// Player economy and supply vitals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerResources {
    /// Mineral resource reserve.
    pub minerals: u32,
    /// Vespene gas resource reserve.
    pub gas: u32,
    /// Currently consumed army/worker supply.
    pub supply_used: u32,
    /// Maximum supply cap provided by Command and Supply Domes (capped at 200).
    pub supply_max: u32,
}

impl Default for PlayerResources {
    fn default() -> Self {
        Self {
            minerals: 1000,
            gas: 500,
            supply_used: 0,
            supply_max: 200,
        }
    }
}

/// Active continuous hazard/AoE zone (e.g. Psionic Storm) ticking in the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveHazardZone {
    /// Monotonically increasing hazard ID.
    pub id: u32,
    /// Player owner handle.
    pub owner: PlayerId,
    /// Center world coordinates.
    pub center: SimVec2,
    /// Circular radius of effect.
    pub radius: SimScalar,
    /// Initial detonation delay in ticks before first damage application.
    pub delay_ticks: u16,
    /// Total lifetime remaining ticks.
    pub remaining_ticks: u16,
    /// Ticks between consecutive periodic damage pulses.
    pub interval_ticks: u16,
    /// Ticks remaining until the next damage pulse.
    pub ticks_until_next_interval: u16,
    /// Damage applied per interval pulse.
    pub damage_per_interval: SimScalar,
    /// If true, friendly entities also suffer damage.
    pub targets_friendly: bool,
}

/// The authoritative simulation world for Space Scuffle.
pub struct SimWorld {
    /// Monotonically increasing tick counter (25 ticks per second).
    pub current_tick: u32,
    /// Authoritative deterministic PRNG.
    pub rng: ChaCha8Rng,
    /// Seed used for checksum hashing and determinism tracking.
    pub rng_seed: u64,
    /// Authoritative active entities strictly sorted by `EntityId`.
    pub entities: BTreeMap<EntityId, Entity>,
    /// In-flight physical projectiles.
    pub projectiles: Vec<ActiveProjectile>,
    /// Monotonically increasing entity ID generator.
    pub next_entity_id: u32,
    /// Monotonically increasing projectile ID generator.
    pub next_projectile_id: u32,
    /// Static terrain grid, cliffs, and placed building footprint blocks.
    pub map: SimMap,
    /// Dynamic clearance A* pathfinder and Simple Stupid Funnel string-puller.
    pub pathfinder: Pathfinder,
    /// Per-player Fog of War visibility maps (up to 8 players).
    pub vision_maps: [PlayerVisionMap; 8],
    /// Static archetype specifications.
    pub catalog: UnitCatalog,
    /// Static upgrade specifications and effects catalog.
    pub upgrade_catalog: UpgradeCatalog,
    /// Per-player active upgrade state (up to 8 players).
    pub player_upgrades: [PlayerUpgradeState; 8],
    /// Flat 2D spatial hash accelerating neighborhood queries.
    pub spatial_hash: SpatialHashGrid,
    /// Per-player economic resources and supply caps.
    pub player_resources: [PlayerResources; 8],
    /// Checksum calculated at the conclusion of the previous tick.
    pub last_checksum: u64,
    /// Transient visual events generated during the current tick for renderer consumption.
    pub pending_events: Vec<CombatEventVisual>,
    /// Active continuous hazard/AoE zones (e.g. Psionic Storm) ticking in the world.
    pub active_hazards: Vec<ActiveHazardZone>,
    /// Monotonically increasing hazard ID generator.
    pub next_hazard_id: u32,
    /// Whether pathfinding respects fog-of-war vision and known obstacles.
    pub vision_aware_pathfinding: bool,
    /// Per-player known blocked cells (structures seen in visible cells and remembered through fog).
    pub known_blocked_cells: [Vec<bool>; 8],

    // Reusable per-tick scratch buffers to eliminate allocations
    scratch_neighbors: Vec<EntityId>,
    scratch_targets: Vec<(TargetPriorityKey, EntityId)>,
    scratch_dead_entities: Vec<EntityId>,
}

impl SimWorld {
    /// Constructs a new simulation world on `map` with the given deterministic RNG seed.
    #[must_use]
    pub fn new(map: SimMap, seed: u64) -> Self {
        let width = map.width();
        let height = map.height();
        let pathfinder = Pathfinder::new(&map);
        let spatial_hash = SpatialHashGrid::new(width as usize, height as usize, SPATIAL_CELL_SIZE);

        let mut vision_maps = [
            PlayerVisionMap::new(width, height),
            PlayerVisionMap::new(width, height),
            PlayerVisionMap::new(width, height),
            PlayerVisionMap::new(width, height),
            PlayerVisionMap::new(width, height),
            PlayerVisionMap::new(width, height),
            PlayerVisionMap::new(width, height),
            PlayerVisionMap::new(width, height),
        ];
        for v in &mut vision_maps {
            v.set_all_fogged();
        }

        let total_cells = (width as usize) * (height as usize);
        let known_blocked_cells = [
            vec![false; total_cells],
            vec![false; total_cells],
            vec![false; total_cells],
            vec![false; total_cells],
            vec![false; total_cells],
            vec![false; total_cells],
            vec![false; total_cells],
            vec![false; total_cells],
        ];

        let catalog = UnitCatalog::new_starter_roster();
        let mut world = Self {
            current_tick: 0,
            rng: ChaCha8Rng::seed_from_u64(seed),
            rng_seed: seed,
            entities: BTreeMap::new(),
            projectiles: Vec::new(),
            next_entity_id: 1,
            next_projectile_id: 1,
            map,
            pathfinder,
            vision_maps,
            catalog,
            upgrade_catalog: UpgradeCatalog::default_catalog(),
            player_upgrades: Default::default(),
            spatial_hash,
            player_resources: [PlayerResources::default(); 8],
            last_checksum: 0,
            pending_events: Vec::new(),
            active_hazards: Vec::new(),
            next_hazard_id: 1,
            vision_aware_pathfinding: true,
            known_blocked_cells,
            scratch_neighbors: Vec::with_capacity(32),
            scratch_targets: Vec::with_capacity(64),
            scratch_dead_entities: Vec::with_capacity(16),
        };

        world.last_checksum = world.compute_checksum();
        world
    }

    /// Constructs the standard default battle sandbox used by the client Visual Lab.
    ///
    /// Spawns:
    /// - 64x64 flat arena.
    /// - Blue Squad (Player 0): 10 Marines + 2 Marauders at (15.0, 32.0).
    /// - Red Squad (Player 1): 8 Zealots + 2 Siege Tanks at (45.0, 32.0).
    /// - Blue Base: CommandDome at (5, 5), SupplyDome at (12, 5), BarracksDome at (16, 5).
    /// - Red Base: CommandDome at (54, 54), SupplyDome at (48, 54), BarracksDome at (42, 54).
    ///
    /// # Panics
    /// Panics if standard starter archetypes are missing from the embedded catalog.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn new_default_battle_sandbox() -> Self {
        let default_cell = MapCell::default_ground(CliffLevel(0));
        let map = match GameMap::new(64, 64, default_cell) {
            Ok(m) => m,
            Err(_) => unreachable!("invariant: 64x64 default map dimensions are valid"),
        };
        let mut world = Self::new(map, 0x5EED_BEEF);

        // Resolve sandbox unit IDs dynamically by role/capability from declarative catalog
        let light_ranged_id = world
            .catalog
            .archetypes()
            .find(|a| {
                !matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && a.attributes.contains(AttributeFlags::LIGHT)
                    && a.weapon
                        .as_ref()
                        .is_some_and(|w| w.max_range > SimScalar::from_num(1))
            })
            .map(|a| a.unit_type_id)
            .or_else(|| {
                world
                    .catalog
                    .archetypes()
                    .find(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
                    .map(|a| a.unit_type_id)
            })
            .expect("invariant: light ranged unit archetype exists in catalog");

        let heavy_armored_id = world
            .catalog
            .archetypes()
            .find(|a| {
                !matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && a.attributes.contains(AttributeFlags::ARMORED)
                    && !a.weapon.as_ref().is_some_and(|w| w.has_turret)
            })
            .map_or(light_ranged_id, |a| a.unit_type_id);

        let melee_assault_id = world
            .catalog
            .archetypes()
            .find(|a| {
                !matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && a.capabilities.harvester.is_none()
                    && a.weapon
                        .as_ref()
                        .is_some_and(|w| w.max_range <= SimScalar::from_num(2))
            })
            .map_or(light_ranged_id, |a| a.unit_type_id);

        let siege_turret_id = world
            .catalog
            .archetypes()
            .find(|a| {
                !matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && a.weapon.as_ref().is_some_and(|w| w.has_turret)
            })
            .map_or(heavy_armored_id, |a| a.unit_type_id);

        let command_dome_id = world
            .catalog
            .archetypes()
            .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }) && a.is_resource_dropoff)
            .map(|a| a.unit_type_id)
            .or_else(|| {
                world
                    .catalog
                    .archetypes()
                    .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
                    .map(|a| a.unit_type_id)
            })
            .expect("invariant: command structure archetype exists in catalog");

        let supply_dome_id = world
            .catalog
            .archetypes()
            .find(|a| {
                matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && a.supply_provided > 0
                    && !a.is_resource_dropoff
            })
            .map_or(command_dome_id, |a| a.unit_type_id);

        let production_dome_id = world
            .catalog
            .archetypes()
            .find(|a| {
                matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && !a.is_resource_dropoff
                    && a.supply_provided == 0
                    && a.weapon.is_none()
            })
            .map_or(command_dome_id, |a| a.unit_type_id);

        // Spawn Blue Army (Player 0)
        let blue = PlayerId(0);
        for i in 0..10i32 {
            let y_offset = SimScalar::from_num(i * 2 - 9);
            let pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(32) + y_offset);
            let _ = world.spawn_unit(light_ranged_id, blue, pos);
        }
        for i in 0..2i32 {
            let y_offset = SimScalar::from_num(i * 4 - 2);
            let pos = SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(32) + y_offset);
            let _ = world.spawn_unit(heavy_armored_id, blue, pos);
        }

        // Spawn Red Army (Player 1)
        let red = PlayerId(1);
        for i in 0..8i32 {
            let y_offset = SimScalar::from_num(i * 2 - 7);
            let pos = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(32) + y_offset);
            let _ = world.spawn_unit(melee_assault_id, red, pos);
        }
        for i in 0..2i32 {
            let y_offset = SimScalar::from_num(i * 4 - 2);
            let pos = SimVec2::new(SimScalar::from_num(48), SimScalar::from_num(32) + y_offset);
            let _ = world.spawn_unit(siege_turret_id, red, pos);
        }

        // Spawn Blue Structures
        let _ = world.spawn_structure(command_dome_id, blue, 5, 5);
        let _ = world.spawn_structure(supply_dome_id, blue, 12, 5);
        let _ = world.spawn_structure(production_dome_id, blue, 16, 5);

        // Spawn Red Structures
        let _ = world.spawn_structure(command_dome_id, red, 54, 54);
        let _ = world.spawn_structure(supply_dome_id, red, 48, 54);
        let _ = world.spawn_structure(production_dome_id, red, 42, 54);

        world.last_checksum = world.compute_checksum();
        world
    }

    /// Spawns a mobile unit entity at `pos`.
    ///
    /// # Errors
    /// Returns [`SimCoreError::ArchetypeNotFound`] if `unit_type` is not in the catalog.
    pub fn spawn_unit(
        &mut self,
        unit_type: UnitTypeId,
        owner: PlayerId,
        pos: SimVec2,
    ) -> SimResult<EntityId> {
        let archetype = self.catalog.get(unit_type)?;
        let id = EntityId(self.next_entity_id);
        self.next_entity_id = self.next_entity_id.wrapping_add(1);

        let mut entity = Entity::from_archetype(id, owner, pos, archetype);
        entity.max_hp = self.effective_max_hp(archetype, owner);
        entity.hp = entity.max_hp;
        self.entities.insert(id, entity);
        Ok(id)
    }

    /// Spawns a building foundation dome at cell `(cell_x, cell_y)`.
    ///
    /// # Errors
    /// Returns [`SimCoreError::ArchetypeNotFound`] if `structure_type` is not in catalog.
    /// Returns [`SimCoreError::InvalidBuildingPlacement`] if the footprint is obstructed.
    pub fn spawn_structure(
        &mut self,
        structure_type: UnitTypeId,
        owner: PlayerId,
        cell_x: u32,
        cell_y: u32,
    ) -> SimResult<EntityId> {
        let archetype = self.catalog.get(structure_type)?;
        let ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
        } = archetype.shape
        else {
            return Err(SimCoreError::InvalidCommand(
                EntityId(0),
                "Spawn structure called with non-dome shape".to_string(),
            ));
        };

        let id = EntityId(self.next_entity_id);
        self.next_entity_id = self.next_entity_id.wrapping_add(1);

        self.map
            .place_structure(id, cell_x, cell_y, footprint_w, footprint_h)
            .map_err(|_| SimCoreError::InvalidBuildingPlacement {
                x: cell_x,
                y: cell_y,
            })?;

        self.pathfinder.recompute_clearance(&self.map);

        let center_x =
            SimScalar::from_num(cell_x) + SimScalar::from_num(footprint_w) * SimScalar::HALF;
        let center_y =
            SimScalar::from_num(cell_y) + SimScalar::from_num(footprint_h) * SimScalar::HALF;
        let pos = SimVec2::new(center_x, center_y);

        let mut entity = Entity::from_archetype(id, owner, pos, archetype);
        entity.max_hp = self.effective_max_hp(archetype, owner);
        entity.hp = entity.max_hp;
        entity.is_completed = true;
        self.entities.insert(id, entity);
        Ok(id)
    }

    /// Returns a shared reference to the player's active upgrade state.
    #[must_use]
    pub fn player_upgrade_state(&self, player: PlayerId) -> &PlayerUpgradeState {
        let idx = (player.0 as usize).min(7);
        &self.player_upgrades[idx]
    }

    /// Returns a mutable reference to the player's active upgrade state.
    pub fn player_upgrade_state_mut(&mut self, player: PlayerId) -> &mut PlayerUpgradeState {
        let idx = (player.0 as usize).min(7);
        &mut self.player_upgrades[idx]
    }

    /// Applies a researched upgrade to a player's upgrade state, updating any affected live entities.
    ///
    /// # Errors
    /// Returns [`SimCoreError::UnknownUpgrade`] if the upgrade slug is unknown.
    pub fn research_upgrade(&mut self, player_id: PlayerId, upgrade_slug: &str) -> SimResult<()> {
        let upgrade = self
            .upgrade_catalog
            .get_by_slug(upgrade_slug)
            .ok_or_else(|| SimCoreError::UnknownUpgrade(upgrade_slug.to_string()))?
            .clone();

        self.player_upgrade_state_mut(player_id)
            .apply_upgrade(upgrade_slug, &upgrade.effects);

        // Apply any retroactive entity stat updates (e.g. Combat Shield max HP)
        for effect in &upgrade.effects {
            if let sim_types::UpgradeEffect::ModifyUnitStat {
                target_slug,
                max_hp_delta,
                ..
            } = effect
            {
                if *max_hp_delta > SimScalar::ZERO {
                    for ent in self.entities.values_mut() {
                        if ent.owner == player_id {
                            if let Ok(arch) = self.catalog.get(ent.unit_type_id) {
                                if arch.slug == *target_slug {
                                    ent.max_hp += *max_hp_delta;
                                    ent.hp += *max_hp_delta;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates whether a player possesses all prerequisite structures to construct a building.
    ///
    /// # Errors
    /// Returns [`SimCoreError::InvalidUnitType`] or [`SimCoreError::MissingPrerequisiteStructure`].
    pub fn can_construct_structure(
        &self,
        player_id: PlayerId,
        structure_slug: &str,
    ) -> SimResult<()> {
        crate::tech_tree::TechTreeValidator::can_construct_structure(
            self,
            player_id,
            structure_slug,
        )
    }

    /// Validates whether a player possesses prerequisites to research an upgrade.
    ///
    /// # Errors
    /// Returns [`SimCoreError::UnknownUpgrade`], [`SimCoreError::UpgradeAlreadyResearched`],
    /// or [`SimCoreError::MissingPrerequisiteStructure`].
    pub fn can_research_upgrade(&self, player_id: PlayerId, upgrade_slug: &str) -> SimResult<()> {
        crate::tech_tree::TechTreeValidator::can_research_upgrade(self, player_id, upgrade_slug)
    }

    fn effective_speed_from_upgrades(
        upgrades: &[PlayerUpgradeState; 8],
        archetype: &UnitArchetype,
        owner: PlayerId,
    ) -> SimScalar {
        let p_idx = (owner.0 as usize).min(7);
        let speed_delta = upgrades[p_idx]
            .unit_stat_modifiers
            .get(&archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.move_speed_delta);
        archetype.max_speed + speed_delta
    }

    fn effective_range_from_upgrades(
        upgrades: &[PlayerUpgradeState; 8],
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> SimScalar {
        let p_idx = (owner.0 as usize).min(7);
        let range_delta = upgrades[p_idx]
            .unit_stat_modifiers
            .get(unit_slug)
            .map_or(SimScalar::ZERO, |m| m.weapon_range_delta);
        weapon.max_range + range_delta
    }

    fn effective_period_from_upgrades(
        upgrades: &[PlayerUpgradeState; 8],
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> u16 {
        let p_idx = (owner.0 as usize).min(7);
        let cd_delta = upgrades[p_idx]
            .unit_stat_modifiers
            .get(unit_slug)
            .map_or(0, |m| m.attack_cooldown_delta);
        let period = (weapon.period_ticks as i32 + cd_delta).max(1);
        period as u16
    }

    /// Computes the effective movement speed for an archetype and owner taking into account active upgrades.
    #[must_use]
    pub fn effective_max_speed(&self, archetype: &UnitArchetype, owner: PlayerId) -> SimScalar {
        Self::effective_speed_from_upgrades(&self.player_upgrades, archetype, owner)
    }

    /// Computes the effective weapon range taking into account active upgrades.
    #[must_use]
    pub fn effective_weapon_range(
        &self,
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> SimScalar {
        Self::effective_range_from_upgrades(&self.player_upgrades, weapon, unit_slug, owner)
    }

    /// Computes the effective attack period (cooldown ticks) taking into account active upgrades.
    #[must_use]
    pub fn effective_attack_period(
        &self,
        weapon: &WeaponSpec,
        unit_slug: &str,
        owner: PlayerId,
    ) -> u16 {
        Self::effective_period_from_upgrades(&self.player_upgrades, weapon, unit_slug, owner)
    }

    /// Computes the effective maximum HP taking into account active upgrades.
    #[must_use]
    pub fn effective_max_hp(&self, archetype: &UnitArchetype, owner: PlayerId) -> SimScalar {
        let hp_delta = self
            .player_upgrade_state(owner)
            .unit_stat_modifiers
            .get(&archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.max_hp_delta);
        archetype.max_hp + hp_delta
    }

    /// Advances the deterministic simulation by exactly one 25 Hz (40 ms) tick.
    pub fn step(&mut self, input: &TickBundle) -> RenderSnapshot {
        self.current_tick = self.current_tick.wrapping_add(1);

        self.process_commands(input);
        self.update_production_and_buffs();
        self.update_vision();
        self.acquire_targets();
        self.step_movement_and_steering();
        self.resolve_collisions_and_push();
        self.step_weapons_and_projectiles();
        self.reap_dead_and_emit_shatter();
        self.last_checksum = self.compute_checksum();
        self.build_render_snapshot()
    }

    /// Returns the most recently computed 64-bit state checksum.
    #[must_use]
    #[inline]
    pub fn last_checksum(&self) -> u64 {
        self.last_checksum
    }

    /// Executes an active ability or spell for an entity.
    ///
    /// Validates energy cost, cooldowns, range, terrain pathability, and channeling.
    /// Returns `true` if cast successfully; `false` otherwise.
    pub fn execute_ability(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        let _ = target_entity;
        let entity = match self.entities.get_mut(&entity_id) {
            Some(e) if e.is_alive() => e,
            _ => return false,
        };

        if entity.channeling_remaining_ticks > 0 {
            return false;
        }

        let arch = match self.catalog.get(entity.unit_type_id) {
            Ok(a) => a,
            Err(_) => return false,
        };

        let ability = match arch.abilities.get(ability_index as usize) {
            Some(a) => a,
            None => return false,
        };

        if let Some(&cd) = entity.ability_cooldowns.get(ability_index as usize) {
            if cd > 0 {
                return false;
            }
        }

        if entity.energy < ability.energy_cost {
            return false;
        }

        if ability.cast_range > SimScalar::ZERO {
            if let Some(pos) = target_pos {
                if entity.position.distance(pos) > ability.cast_range {
                    return false;
                }
            }
        }

        // Validate ability preconditions
        match &ability.effect {
            AbilityEffectSpec::Overclock(_) => {
                if entity.hp <= SimScalar::ONE {
                    return false;
                }
            }
            AbilityEffectSpec::SiegeModeToggle { .. }
            | AbilityEffectSpec::CloakToggle { .. }
            | AbilityEffectSpec::SwitchMode { .. } => {}
            AbilityEffectSpec::BlinkTeleport { max_range, .. } => {
                let Some(dest) = target_pos else {
                    return false;
                };
                if entity.position.distance(dest) > *max_range {
                    return false;
                }
                let Ok((cx, cy)) = self.map.world_to_cell(dest) else {
                    return false;
                };
                let Ok(cell) = self.map.cell(cx, cy) else {
                    return false;
                };
                if !cell.flags.contains(CellFlags::GROUND_PATHABLE)
                    || cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
                    || cell.flags.contains(CellFlags::UNPASSABLE_CLIFF)
                {
                    return false;
                }
            }
            AbilityEffectSpec::PsionicStormAoE { .. } => {
                if target_pos.is_none() {
                    return false;
                }
            }
        }

        entity.energy -= ability.energy_cost;
        if (ability_index as usize) < entity.ability_cooldowns.len() {
            entity.ability_cooldowns[ability_index as usize] = ability.cooldown_ticks;
        }

        let effect = ability.effect.clone();
        let entity_owner = entity.owner;
        match effect {
            AbilityEffectSpec::Overclock(spec) => {
                entity.apply_overclock(spec);
            }
            AbilityEffectSpec::SiegeModeToggle {
                cast_ticks,
                sieged_weapon,
            } => {
                if cast_ticks > 0 {
                    entity.is_immobile = true;
                    entity.unpushable = true;
                    entity.channeling_remaining_ticks = cast_ticks;
                    entity.pending_ability = Some((ability_index, target_pos, target_entity));
                    entity.siege_weapon = Some(sieged_weapon);
                    entity.velocity = SimVec2::ZERO;
                    entity.current_funnel_waypoints.clear();
                } else {
                    entity.is_sieged = !entity.is_sieged;
                    entity.is_immobile = entity.is_sieged;
                    entity.unpushable = entity.is_sieged;
                    if entity.is_sieged {
                        entity.active_mode = Some("siege".to_string());
                        entity.siege_weapon = Some(sieged_weapon);
                    } else {
                        entity.active_mode = None;
                        entity.siege_weapon = None;
                    }
                    entity.velocity = SimVec2::ZERO;
                    entity.current_funnel_waypoints.clear();
                }
            }
            AbilityEffectSpec::SwitchMode { mode_name } => {
                let mode_spec = arch.get_mode(&mode_name).cloned();
                if let Some(spec) = mode_spec {
                    if spec.cast_ticks > 0 {
                        entity.is_immobile = true;
                        entity.unpushable = true;
                        entity.channeling_remaining_ticks = spec.cast_ticks;
                        entity.pending_ability = Some((ability_index, target_pos, target_entity));
                        if let Some(Some(ref w)) = spec.ground_weapon_override {
                            entity.siege_weapon = Some(w.clone());
                        }
                        entity.velocity = SimVec2::ZERO;
                        entity.current_funnel_waypoints.clear();
                    } else {
                        let is_currently_active = entity.active_mode.as_deref() == Some(&mode_name);
                        if is_currently_active {
                            entity.active_mode = None;
                            let is_immobile_default = arch.is_immobile_by_default();
                            entity.is_immobile = is_immobile_default;
                            entity.unpushable = is_immobile_default;
                            entity.is_cloaked = false;
                            entity.siege_weapon = None;
                            entity.is_sieged = false;
                        } else {
                            entity.active_mode = Some(mode_name);
                            entity.is_immobile = spec.is_immobile;
                            entity.unpushable = spec.unpushable;
                            entity.is_cloaked = spec.is_cloaked;
                            if let Some(ref w_opt) = spec.ground_weapon_override {
                                entity.siege_weapon.clone_from(w_opt);
                                entity.is_sieged = w_opt.is_some();
                            } else {
                                entity.is_sieged = false;
                                entity.siege_weapon = None;
                            }
                        }
                        entity.velocity = SimVec2::ZERO;
                        entity.current_funnel_waypoints.clear();
                    }
                }
            }
            AbilityEffectSpec::BlinkTeleport { .. } => {
                if let Some(dest) = target_pos {
                    entity.position = dest;
                    entity.velocity = SimVec2::ZERO;
                    entity.current_funnel_waypoints.clear();
                }
            }
            AbilityEffectSpec::PsionicStormAoE {
                radius,
                delay_ticks,
                duration_ticks,
                interval_ticks,
                damage_per_interval,
                friendly_fire,
            } => {
                if let Some(pos) = target_pos {
                    let h_id = self.next_hazard_id;
                    self.next_hazard_id = self.next_hazard_id.wrapping_add(1);
                    self.active_hazards.push(ActiveHazardZone {
                        id: h_id,
                        owner: entity_owner,
                        center: pos,
                        radius,
                        delay_ticks,
                        remaining_ticks: duration_ticks,
                        interval_ticks,
                        ticks_until_next_interval: delay_ticks,
                        damage_per_interval,
                        targets_friendly: friendly_fire,
                    });
                }
            }
            AbilityEffectSpec::CloakToggle { is_burrow, .. } => {
                entity.is_cloaked = !entity.is_cloaked;
                if is_burrow {
                    entity.is_immobile = entity.is_cloaked;
                    entity.unpushable = entity.is_cloaked;
                    if entity.is_cloaked {
                        entity.velocity = SimVec2::ZERO;
                        entity.current_funnel_waypoints.clear();
                    }
                }
            }
        }

        true
    }

    // =========================================================================
    // 10-Phase Deterministic Pipeline Implementation
    // =========================================================================

    /// Phase 1: Process scheduled player input commands.
    pub fn process_commands(&mut self, input: &TickBundle) {
        for cmd in &input.commands {
            let player_idx = cmd.player_id.0 as usize;
            if player_idx >= self.player_resources.len() {
                continue;
            }

            match &cmd.order {
                UnitOrder::Produce { unit_type } => {
                    if let Ok(arch) = self.catalog.get(*unit_type) {
                        let res = &mut self.player_resources[player_idx];
                        if res.minerals >= arch.mineral_cost && res.gas >= arch.gas_cost {
                            res.minerals -= arch.mineral_cost;
                            res.gas -= arch.gas_cost;

                            for &ent_id in &cmd.entities {
                                if let Some(ent) = self.entities.get_mut(&ent_id) {
                                    if ent.owner == cmd.player_id {
                                        ent.production_queue
                                            .push_back((*unit_type, arch.build_time_ticks as u16));
                                        break; // Queue into first matching structure
                                    }
                                }
                            }
                        }
                    }
                }
                UnitOrder::Build {
                    structure_type,
                    cell_x,
                    cell_y,
                } => {
                    let _ = self.spawn_structure(*structure_type, cmd.player_id, *cell_x, *cell_y);
                }
                UnitOrder::UseStim => {
                    for &ent_id in &cmd.entities {
                        if let Some(ent) = self.entities.get_mut(&ent_id) {
                            if ent.owner == cmd.player_id {
                                if let Ok(arch) = self.catalog.get(ent.unit_type_id) {
                                    if let Some(spec) = arch.overclock {
                                        let _ = ent.apply_overclock(spec);
                                    }
                                }
                            }
                        }
                    }
                }
                UnitOrder::CastAbility {
                    ability_index,
                    target_pos,
                    target_entity,
                } => {
                    for &ent_id in &cmd.entities {
                        if let Some(ent) = self.entities.get_mut(&ent_id) {
                            if ent.owner == cmd.player_id {
                                if cmd.queue && !ent.order_queue.is_empty() {
                                    ent.issue_order(cmd.order.clone(), true);
                                } else {
                                    self.execute_ability(
                                        ent_id,
                                        *ability_index,
                                        *target_pos,
                                        *target_entity,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {
                    for &ent_id in &cmd.entities {
                        if let Some(ent) = self.entities.get_mut(&ent_id) {
                            if ent.owner == cmd.player_id {
                                ent.issue_order(cmd.order.clone(), cmd.queue);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Phase 2: Update production timers, buffs, passive regeneration, and shield delays.
    pub fn update_production_and_buffs(&mut self) {
        let mut completed_spawns = Vec::new();
        let catalog = &self.catalog;

        for entity in self.entities.values_mut() {
            // Channeling timers
            if entity.channeling_remaining_ticks > 0 {
                entity.channeling_remaining_ticks =
                    entity.channeling_remaining_ticks.saturating_sub(1);
                if entity.channeling_remaining_ticks == 0 {
                    if let Some((ab_idx, _t_pos, _t_ent)) = entity.pending_ability.take() {
                        if let Ok(arch) = catalog.get(entity.unit_type_id) {
                            if let Some(ab) = arch.abilities.get(ab_idx as usize) {
                                match &ab.effect {
                                    AbilityEffectSpec::SwitchMode { mode_name } => {
                                        if let Some(spec) = arch.get_mode(mode_name) {
                                            let is_currently_active = entity.active_mode.as_deref()
                                                == Some(mode_name.as_str());
                                            if is_currently_active {
                                                entity.active_mode = None;
                                                let is_immobile_default =
                                                    arch.is_immobile_by_default();
                                                entity.is_immobile = is_immobile_default;
                                                entity.unpushable = is_immobile_default;
                                                entity.is_cloaked = false;
                                                entity.siege_weapon = None;
                                                entity.is_sieged = false;
                                            } else {
                                                entity.active_mode = Some(mode_name.clone());
                                                entity.is_immobile = spec.is_immobile;
                                                entity.unpushable = spec.unpushable;
                                                entity.is_cloaked = spec.is_cloaked;
                                                if let Some(ref w_opt) = spec.ground_weapon_override
                                                {
                                                    entity.siege_weapon.clone_from(w_opt);
                                                    entity.is_sieged = w_opt.is_some();
                                                }
                                            }
                                        }
                                    }
                                    AbilityEffectSpec::SiegeModeToggle { .. } => {
                                        entity.is_sieged = !entity.is_sieged;
                                        entity.is_immobile = entity.is_sieged;
                                        entity.unpushable = entity.is_sieged;
                                        if entity.is_sieged {
                                            entity.active_mode = Some("siege".to_string());
                                        } else {
                                            entity.active_mode = None;
                                            entity.siege_weapon = None;
                                        }
                                    }
                                    _ => {}
                                }
                            } else {
                                entity.is_sieged = !entity.is_sieged;
                                entity.is_immobile = entity.is_sieged;
                                entity.unpushable = entity.is_sieged;
                                if entity.is_sieged {
                                    entity.active_mode = Some("siege".to_string());
                                } else {
                                    entity.active_mode = None;
                                    entity.siege_weapon = None;
                                }
                            }
                        } else {
                            entity.is_sieged = !entity.is_sieged;
                            entity.is_immobile = entity.is_sieged;
                            entity.unpushable = entity.is_sieged;
                            if entity.is_sieged {
                                entity.active_mode = Some("siege".to_string());
                            } else {
                                entity.active_mode = None;
                                entity.siege_weapon = None;
                            }
                        }
                    }
                }
            }

            // Ability cooldowns
            for cd in &mut entity.ability_cooldowns {
                *cd = cd.saturating_sub(1);
            }

            // Production timers
            if let Some(front) = entity.production_queue.front_mut() {
                front.1 = front.1.saturating_sub(1);
                if front.1 == 0 {
                    let unit_type = front.0;
                    completed_spawns.push((unit_type, entity.owner, entity.position));
                    entity.production_queue.pop_front();
                }
            }

            // Buff timers
            for buff in &mut entity.active_buffs {
                buff.remaining_ticks = buff.remaining_ticks.saturating_sub(1);
            }
            entity.active_buffs.retain(|b| b.remaining_ticks > 0);

            // Passive Vitals & Shield delay
            if let Ok(arch) = catalog.get(entity.unit_type_id) {
                entity.hp = (entity.hp + arch.hp_regen_per_tick).min(arch.max_hp);
                entity.energy = (entity.energy + arch.energy_regen_per_tick).min(arch.max_energy);

                // Continuous energy drain for active stance/mode
                if let Some(ref mode_name) = entity.active_mode {
                    if let Some(mode) = arch.get_mode(mode_name) {
                        if mode.energy_cost_per_second > SimScalar::ZERO {
                            let drain_per_tick =
                                mode.energy_cost_per_second / SimScalar::from_i32(25);
                            if entity.energy < drain_per_tick {
                                entity.energy = SimScalar::ZERO;
                                entity.active_mode = None;
                                let is_immobile_default = arch.is_immobile_by_default();
                                entity.is_immobile = is_immobile_default;
                                entity.unpushable = is_immobile_default;
                                entity.is_cloaked = false;
                                entity.siege_weapon = None;
                                entity.is_sieged = false;
                            } else {
                                entity.energy -= drain_per_tick;
                            }
                        }
                    }
                }

                entity.ticks_since_last_damage = entity.ticks_since_last_damage.saturating_add(1);
                if entity.ticks_since_last_damage >= 250 {
                    entity.shields =
                        (entity.shields + arch.shield_regen_per_tick).min(arch.max_shields);
                }
            }

            // Hit flash timers
            entity.shield_hit_flash_ticks = entity.shield_hit_flash_ticks.saturating_sub(1);
            entity.hull_hit_flash_ticks = entity.hull_hit_flash_ticks.saturating_sub(1);
        }

        // Step active hazard zones (e.g. Psionic Storm)
        let cur_tick = self.current_tick;
        let mut expired_hazards = Vec::new();
        for (h_idx, hazard) in self.active_hazards.iter_mut().enumerate() {
            if hazard.remaining_ticks == 0 {
                expired_hazards.push(h_idx);
                continue;
            }
            hazard.remaining_ticks = hazard.remaining_ticks.saturating_sub(1);

            if hazard.ticks_until_next_interval > 0 {
                hazard.ticks_until_next_interval =
                    hazard.ticks_until_next_interval.saturating_sub(1);
            }

            if hazard.ticks_until_next_interval == 0 {
                hazard.ticks_until_next_interval = hazard.interval_ticks;

                for entity in self.entities.values_mut() {
                    if !entity.is_alive() {
                        continue;
                    }
                    if !hazard.targets_friendly && entity.owner == hazard.owner {
                        continue;
                    }
                    if entity.last_storm_damage_tick == Some(Tick(cur_tick)) {
                        continue;
                    }
                    let dist = hazard.center.distance(entity.position);
                    if dist <= hazard.radius {
                        let dmg = hazard.damage_per_interval;
                        let (shield_dmg, life_dmg) = if entity.shields >= dmg {
                            (dmg, SimScalar::ZERO)
                        } else {
                            let s = entity.shields;
                            (s, dmg - s)
                        };
                        entity.apply_damage(shield_dmg, life_dmg, SimAngle::ZERO);
                        entity.last_storm_damage_tick = Some(Tick(cur_tick));
                    }
                }
            }
        }
        for h_idx in expired_hazards.into_iter().rev() {
            self.active_hazards.remove(h_idx);
        }

        // Spawn completed production units
        for (unit_type, owner, pos) in completed_spawns {
            let spawn_pos = pos + SimVec2::new(SimScalar::from_num(2), SimScalar::ZERO);
            let _ = self.spawn_unit(unit_type, owner, spawn_pos);
        }
    }

    /// Phase 3: Update per-player Fog of War visibility maps and remembered obstacle perception.
    pub fn update_vision(&mut self) {
        for p in 0..8 {
            self.vision_maps[p].begin_tick_update();
        }

        for entity in self.entities.values() {
            if let Ok(arch) = self.catalog.get(entity.unit_type_id) {
                let p = entity.owner.0 as usize;
                if p < 8 {
                    let source = VisionSource {
                        pos: entity.position,
                        mover_type: arch.mover_type,
                        sight_radius: arch.sight_radius,
                        is_detector: arch.is_detector,
                    };
                    self.vision_maps[p].apply_vision_source(&self.map, &source);
                }
            }
        }

        for p in 0..8 {
            self.vision_maps[p].apply_watchtowers(&self.map, PlayerId(p as u8));
            self.vision_maps[p].end_tick_update();
        }

        let width = self.map.width() as usize;
        let height = self.map.height() as usize;
        let total_cells = width * height;

        let mut active_players = [false; 8];
        for ent in self.entities.values() {
            let p = ent.owner.0 as usize;
            if p < 8 {
                active_players[p] = true;
            }
        }

        for (p, &is_active) in active_players.iter().enumerate() {
            if !is_active {
                continue;
            }
            if self.known_blocked_cells[p].len() != total_cells {
                self.known_blocked_cells[p].resize(total_cells, false);
            }
            for cy in 0..height {
                for cx in 0..width {
                    if self.vision_maps[p].visibility_at(cx as u32, cy as u32)
                        == VisibilityState::Visible
                    {
                        let idx = cy * width + cx;
                        if let Ok(cell) = self.map.cell(cx as u32, cy as u32) {
                            self.known_blocked_cells[p][idx] =
                                cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE);
                        }
                    }
                }
            }
        }
    }

    /// Loads a unit into a transport craft, hiding it and disjointing incoming homing projectiles.
    pub fn load_unit_into_transport(&mut self, unit_id: EntityId, _transport_id: EntityId) -> bool {
        if let Some(unit) = self.entities.get_mut(&unit_id) {
            unit.is_loaded_in_transport = true;
            unit.order_queue.clear();
            unit.velocity = SimVec2::ZERO;
            unit.current_target = None;
            unit.current_funnel_waypoints.clear();
            true
        } else {
            false
        }
    }

    /// Unloads a unit from a transport craft at the specified landing position.
    pub fn unload_unit_from_transport(&mut self, unit_id: EntityId, unload_pos: SimVec2) -> bool {
        if let Some(unit) = self.entities.get_mut(&unit_id) {
            unit.is_loaded_in_transport = false;
            unit.position = unload_pos;
            unit.velocity = SimVec2::ZERO;
            unit.order_queue.clear();
            true
        } else {
            false
        }
    }

    /// Checks whether an entity is available to be tracked, acquired, or hit in combat
    /// Checks whether an entity is available to be tracked, acquired, or hit in combat
    /// by an attacker belonging to `attacker_owner` without whole-struct borrowing.
    #[must_use]
    pub fn is_target_available_for_combat(
        entities: &BTreeMap<EntityId, Entity>,
        map: &SimMap,
        vision_maps: &[PlayerVisionMap; 8],
        attacker_owner: PlayerId,
        target_id: EntityId,
    ) -> bool {
        let Some(target) = entities.get(&target_id) else {
            return false;
        };

        if !target.is_alive() || target.is_loaded_in_transport || target.is_invulnerable {
            return false;
        }

        if target.is_cloaked {
            let p_idx = attacker_owner.0 as usize;
            if let Ok((cx, cy)) = map.world_to_cell(target.position) {
                if p_idx < vision_maps.len() {
                    let is_vis =
                        vision_maps[p_idx].visibility_at(cx, cy) == VisibilityState::Visible;
                    let is_det = vision_maps[p_idx].is_detected(cx, cy);
                    if !is_vis || !is_det {
                        return false;
                    }
                } else {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Checks whether an entity is available to be tracked, acquired, or hit in combat
    /// by an attacker belonging to `attacker_owner`.
    ///
    /// An entity is available if and only if:
    /// - It exists and is alive (`hp > 0`),
    /// - It is not loaded in a transport craft,
    /// - It is not invulnerable (e.g. stasis, phase shift),
    /// - If cloaked, the attacker's player possesses active detector vision at the entity's map cell.
    #[must_use]
    #[inline]
    pub fn is_entity_available_for_combat(
        &self,
        attacker_owner: PlayerId,
        target_id: EntityId,
    ) -> bool {
        Self::is_target_available_for_combat(
            &self.entities,
            &self.map,
            &self.vision_maps,
            attacker_owner,
            target_id,
        )
    }

    /// Triggers combat retaliation aggro and allied call-for-help when a unit sustains damage.
    pub fn apply_combat_retaliation(&mut self, victim_id: EntityId, attacker_id: EntityId) {
        let victim_data = self.entities.get(&victim_id).map(|v| (v.position, v.owner));
        let Some((victim_pos, victim_owner)) = victim_data else {
            return;
        };

        let attacker_data = self
            .entities
            .get(&attacker_id)
            .map(|a| (a.is_alive(), a.unit_type_id, a.is_cloaked, a.position));
        let Some((att_alive, att_type_id, att_cloaked, att_pos)) = attacker_data else {
            return;
        };
        if !att_alive {
            return;
        }

        let Ok(att_arch) = self.catalog.get(att_type_id) else {
            return;
        };

        let p_idx = victim_owner.0 as usize;
        // Check visibility of attacker to victim's owner
        if p_idx < 8 {
            if let Ok((cx, cy)) = self.map.world_to_cell(att_pos) {
                let is_vis =
                    self.vision_maps[p_idx].visibility_at(cx, cy) == VisibilityState::Visible;
                if !is_vis {
                    return;
                }
                if att_cloaked && !self.vision_maps[p_idx].is_detected(cx, cy) {
                    return;
                }
            } else {
                return;
            }
        }

        let mut alerted_units = Vec::new();
        let call_radius_sq = SimScalar::from_num(25); // 5.0 world units squared

        for (&id, entity) in &self.entities {
            if entity.owner != victim_owner
                || !entity.is_alive()
                || entity.is_holding_position
                || entity.is_immobile
                || entity.is_loaded_in_transport
            {
                continue;
            }
            // Only alert units that are Idle
            let can_retaliate = matches!(entity.order_queue.front(), None | Some(UnitOrder::Idle));
            if !can_retaliate {
                continue;
            }
            if id != victim_id && entity.position.distance_sq(victim_pos) > call_radius_sq {
                continue;
            }
            // Check if unit has a weapon capable of hitting the attacker
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };
            let active_weapon = arch.weapon_against(
                att_arch.mover_type,
                entity
                    .is_sieged
                    .then_some(entity.siege_weapon.as_ref())
                    .flatten(),
            );
            let Some(weapon) = active_weapon else {
                continue;
            };
            if !weapon.target_flags.intersects(att_arch.attributes) {
                continue;
            }
            alerted_units.push(id);
        }

        for unit_id in alerted_units {
            if let Some(unit) = self.entities.get_mut(&unit_id) {
                unit.order_queue.clear();
                unit.order_queue.push_back(UnitOrder::AttackTarget {
                    target: attacker_id,
                });
                unit.current_target = Some(attacker_id);
                unit.current_funnel_waypoints.clear();
            }
        }
    }

    /// Phase 4: Deterministic target acquisition.
    pub fn acquire_targets(&mut self) {
        // Populate spatial hash grid with current entity positions
        self.spatial_hash.clear();
        for (id, entity) in &self.entities {
            if !entity.is_loaded_in_transport {
                self.spatial_hash.insert(*id, entity.position);
            }
        }

        let mut auto_orders = Vec::new();

        for (id, entity) in &self.entities {
            if entity.channeling_remaining_ticks > 0 || entity.is_loaded_in_transport {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };

            // Evaluate if unit needs to acquire or dynamically re-evaluate target
            let needs_target = match entity.order_queue.front() {
                None | Some(UnitOrder::Idle | UnitOrder::AttackMove { .. }) => true,
                Some(UnitOrder::AttackTarget { target }) => {
                    if entity.order_queue.len() >= 2 {
                        true
                    } else if let Some(current_t) = self.entities.get(target) {
                        if !current_t.is_alive() || current_t.is_loaded_in_transport {
                            true
                        } else if let Ok(t_arch) = self.catalog.get(current_t.unit_type_id) {
                            let active_w = arch.weapon_against(
                                t_arch.mover_type,
                                entity
                                    .is_sieged
                                    .then_some(entity.siege_weapon.as_ref())
                                    .flatten(),
                            );
                            if let Some(w) = active_w {
                                let w_max =
                                    self.effective_weapon_range(w, &arch.slug, entity.owner);
                                let eff_max =
                                    w_max + arch.collision_radius + t_arch.collision_radius;
                                entity.position.distance(current_t.position) > eff_max
                            } else {
                                true
                            }
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                }
                _ => false,
            };

            if !needs_target {
                continue;
            }

            self.spatial_hash.query_radius(
                entity.position,
                arch.scan_range,
                &mut self.scratch_neighbors,
            );

            self.scratch_targets.clear();
            let p_idx = entity.owner.0 as usize;

            for &neighbor_id in &self.scratch_neighbors {
                if neighbor_id == *id {
                    continue;
                }
                let Some(neighbor) = self.entities.get(&neighbor_id) else {
                    continue;
                };

                // Hostile check
                if neighbor.owner == entity.owner
                    || !neighbor.is_alive()
                    || neighbor.is_loaded_in_transport
                {
                    continue;
                }

                // Strict scan range check
                let dist = entity.position.distance(neighbor.position);
                if dist > arch.scan_range {
                    continue;
                }

                let (cx, cy) = match self.map.world_to_cell(neighbor.position) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                // Visibility and detection checks
                if p_idx < 8 {
                    let is_vis =
                        self.vision_maps[p_idx].visibility_at(cx, cy) == VisibilityState::Visible;
                    if !is_vis {
                        continue;
                    }
                    if neighbor.is_cloaked && !self.vision_maps[p_idx].is_detected(cx, cy) {
                        continue;
                    }
                } else if neighbor.is_cloaked {
                    continue;
                }

                let Ok(target_arch) = self.catalog.get(neighbor.unit_type_id) else {
                    continue;
                };

                let active_weapon = arch.weapon_against(
                    target_arch.mover_type,
                    entity
                        .is_sieged
                        .then_some(entity.siege_weapon.as_ref())
                        .flatten(),
                );
                let Some(weapon) = active_weapon else {
                    continue;
                };

                // Weapon target compatibility
                if !weapon.target_flags.intersects(target_arch.attributes) {
                    continue;
                }

                let effective_min = if weapon.min_range > SimScalar::ZERO {
                    weapon.min_range + arch.collision_radius + target_arch.collision_radius
                } else {
                    SimScalar::ZERO
                };
                if weapon.min_range > SimScalar::ZERO && dist < effective_min {
                    continue;
                }

                let dist_sq = entity.position.distance_sq(neighbor.position);
                let priority = compute_threat_priority(target_arch, neighbor.order_queue.front());

                self.scratch_targets.push((
                    TargetPriorityKey {
                        threat_priority: priority,
                        distance_sq: dist_sq,
                        entity_id: neighbor_id,
                    },
                    neighbor_id,
                ));
            }

            if !self.scratch_targets.is_empty() {
                self.scratch_targets.sort_unstable_by_key(|t| t.0);
                let best_target = self.scratch_targets[0].1;
                auto_orders.push((*id, best_target));
            }
        }

        for (unit_id, target_id) in auto_orders {
            if let Some(unit) = self.entities.get_mut(&unit_id) {
                unit.current_target = Some(target_id);
                match unit.order_queue.front() {
                    Some(UnitOrder::AttackMove { .. }) => {
                        unit.order_queue
                            .push_front(UnitOrder::AttackTarget { target: target_id });
                        unit.current_funnel_waypoints.clear();
                    }
                    Some(UnitOrder::AttackTarget { target }) if *target != target_id => {
                        if unit.order_queue.len() >= 2 {
                            unit.order_queue[0] = UnitOrder::AttackTarget { target: target_id };
                            unit.current_funnel_waypoints.clear();
                        } else {
                            unit.order_queue.clear();
                            unit.order_queue
                                .push_back(UnitOrder::AttackTarget { target: target_id });
                            unit.current_funnel_waypoints.clear();
                        }
                    }
                    None | Some(UnitOrder::Idle) => {
                        unit.order_queue.clear();
                        unit.order_queue
                            .push_back(UnitOrder::AttackTarget { target: target_id });
                        unit.current_funnel_waypoints.clear();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Phase 5: Advance movement along NavMesh funnel paths and steer units.
    pub fn step_movement_and_steering(&mut self) {
        let mut cast_abilities = Vec::new();
        let mut target_updates = Vec::new();

        let mut obstacles = Vec::with_capacity(self.entities.len());
        for (&id, entity) in &self.entities {
            if entity.is_alive() && !entity.is_loaded_in_transport {
                let radius = self
                    .catalog
                    .get(entity.unit_type_id)
                    .map_or(SimScalar::HALF, |a| a.collision_radius);
                obstacles.push(SteeringObstacle {
                    id,
                    pos: entity.position,
                    radius,
                    velocity: entity.velocity,
                });
            }
        }

        for entity in self.entities.values_mut() {
            entity.pos_at_tick_start = entity.position;
        }

        for (id, entity) in &mut self.entities {
            if !entity.is_alive()
                || entity.is_holding_position
                || entity.is_immobile
                || entity.is_loaded_in_transport
                || entity.channeling_remaining_ticks > 0
            {
                entity.velocity = SimVec2::ZERO;
                continue;
            }

            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };

            let has_turret = entity
                .siege_weapon
                .as_ref()
                .or(arch.ground_weapon.as_ref())
                .or(arch.weapon.as_ref())
                .is_some_and(|w| w.has_turret);

            if matches!(entity.weapon_phase, WeaponPhase::Windup { .. }) && !has_turret {
                entity.velocity = SimVec2::ZERO;
                continue;
            }

            let speed =
                Self::effective_speed_from_upgrades(&self.player_upgrades, arch, entity.owner)
                    * entity.speed_multiplier();

            match entity.order_queue.front() {
                Some(UnitOrder::CastAbility {
                    ability_index,
                    target_pos,
                    target_entity,
                }) => {
                    cast_abilities.push((*id, *ability_index, *target_pos, *target_entity));
                }
                Some(UnitOrder::Move { target } | UnitOrder::AttackMove { target }) => {
                    let target_pos = *target;
                    let p_idx = entity.owner.0 as usize;

                    // Dynamic repath check if planned route intersects newly scouted obstacles
                    if self.vision_aware_pathfinding
                        && p_idx < 8
                        && !entity.current_funnel_waypoints.is_empty()
                    {
                        let width = self.map.width() as usize;
                        let known_blocked = &self.known_blocked_cells[p_idx];
                        let mut obstructed = false;
                        let mut prev = entity.position;
                        for &wp in &entity.current_funnel_waypoints {
                            let dist = prev.distance(wp);
                            let steps = (dist.0.to_num::<i32>() + 1).max(1);
                            for s in 0..=steps {
                                let t = SimScalar::from_num(s) / SimScalar::from_num(steps);
                                let pt = prev + (wp - prev) * t;
                                if let Ok((cx, cy)) = self.map.world_to_cell(pt) {
                                    let idx = (cy as usize) * width + (cx as usize);
                                    if idx < known_blocked.len() && known_blocked[idx] {
                                        obstructed = true;
                                        break;
                                    }
                                }
                            }
                            if obstructed {
                                break;
                            }
                            prev = wp;
                        }
                        if obstructed {
                            entity.current_funnel_waypoints.clear();
                        }
                    }

                    if entity.current_funnel_waypoints.is_empty() {
                        let dist_to_target = entity.position.distance(target_pos);
                        if dist_to_target <= speed {
                            entity.position = target_pos;
                            entity.velocity = SimVec2::ZERO;
                            entity.order_queue.pop_front();
                            entity.current_anim = AnimState::Idle;
                            continue;
                        }

                        // Check cluster arrival: if in final approach and touching an already settled unit
                        if check_cluster_arrival(
                            entity.position,
                            arch.collision_radius,
                            target_pos,
                            &obstacles,
                        ) {
                            entity.velocity = SimVec2::ZERO;
                            entity.order_queue.pop_front();
                            entity.current_funnel_waypoints.clear();
                            entity.current_anim = AnimState::Idle;
                            continue;
                        }

                        let clearance = if arch.collision_radius <= SimScalar::from_num(0.4) {
                            sim_map::UnitClearance::Small
                        } else if arch.collision_radius <= SimScalar::from_num(0.8) {
                            sim_map::UnitClearance::Medium
                        } else {
                            sim_map::UnitClearance::Large
                        };

                        let req = PathRequestWithVision {
                            start: entity.position,
                            goal: target_pos,
                            clearance,
                            vision: if self.vision_aware_pathfinding && p_idx < 8 {
                                Some(&self.vision_maps[p_idx])
                            } else {
                                None
                            },
                            block_unexplored_ramps: false,
                            known_structures: if self.vision_aware_pathfinding && p_idx < 8 {
                                Some(&self.known_blocked_cells[p_idx])
                            } else {
                                None
                            },
                        };

                        if let Ok(path_res) = self.pathfinder.find_path_with_vision(&self.map, &req)
                        {
                            let mut wps = path_res.waypoints;
                            if let Some(first) = wps.first() {
                                if first.distance(entity.position) <= SimScalar::from_num(0.2) {
                                    wps.remove(0);
                                }
                            }
                            entity.current_funnel_waypoints = wps;
                        }
                    }

                    let next_target = entity
                        .current_funnel_waypoints
                        .first()
                        .copied()
                        .unwrap_or(target_pos);
                    let delta = next_target - entity.position;
                    let dist = delta.length();

                    if dist <= speed {
                        entity.position = next_target;
                        if !entity.current_funnel_waypoints.is_empty() {
                            entity.current_funnel_waypoints.remove(0);
                        }
                        if entity.current_funnel_waypoints.is_empty()
                            && entity.position.distance(target_pos) <= speed
                        {
                            entity.position = target_pos;
                            entity.velocity = SimVec2::ZERO;
                            entity.order_queue.pop_front();
                            entity.current_anim = AnimState::Idle;
                        } else {
                            entity.current_anim = AnimState::Walk;
                        }
                    } else {
                        let steer_dir = compute_unified_local_steering(
                            *id,
                            entity.position,
                            arch.collision_radius,
                            next_target,
                            None,
                            &obstacles,
                        );
                        let vel = steer_dir * speed;
                        let next_pos = entity.position + vel;

                        if Self::is_position_pathable(&self.map, entity.position, next_pos) {
                            entity.velocity = vel;
                            entity.position = next_pos;
                            entity.heading = SimAngle::atan2(entity.velocity.y, entity.velocity.x);
                            entity.current_anim = AnimState::Walk;
                        } else {
                            let slide_x = SimVec2::new(next_pos.x, entity.position.y);
                            if Self::is_position_pathable(&self.map, entity.position, slide_x) {
                                entity.velocity = SimVec2::new(vel.x, SimScalar::ZERO);
                                entity.position = slide_x;
                                entity.heading =
                                    SimAngle::atan2(entity.velocity.y, entity.velocity.x);
                                entity.current_anim = AnimState::Walk;
                            } else {
                                let slide_y = SimVec2::new(entity.position.x, next_pos.y);
                                if Self::is_position_pathable(&self.map, entity.position, slide_y) {
                                    entity.velocity = SimVec2::new(SimScalar::ZERO, vel.y);
                                    entity.position = slide_y;
                                    entity.heading =
                                        SimAngle::atan2(entity.velocity.y, entity.velocity.x);
                                    entity.current_anim = AnimState::Walk;
                                } else {
                                    entity.velocity = SimVec2::ZERO;
                                }
                            }
                        }
                    }
                }
                Some(UnitOrder::AttackTarget { target }) => {
                    let target_id = *target;
                    target_updates.push((*id, target_id));
                }
                _ => {
                    entity.velocity = SimVec2::ZERO;
                    if entity.current_anim == AnimState::Walk {
                        entity.current_anim = AnimState::Idle;
                    }
                }
            }
        }

        for (ent_id, ab_idx, t_pos, t_ent) in cast_abilities {
            if let Some(ent) = self.entities.get_mut(&ent_id) {
                ent.order_queue.pop_front();
            }
            self.execute_ability(ent_id, ab_idx, t_pos, t_ent);
        }

        // Resolve AttackTarget pursuits
        for (attacker_id, target_id) in target_updates {
            let target_data = self.entities.get(&target_id).map(|t| {
                (
                    t.position,
                    t.is_alive(),
                    t.unit_type_id,
                    t.is_cloaked,
                    t.is_loaded_in_transport,
                    t.owner,
                )
            });

            let Some((target_pos, _, target_type, _, _, _)) = target_data else {
                if let Some(att) = self.entities.get_mut(&attacker_id) {
                    att.order_queue.pop_front();
                    att.velocity = SimVec2::ZERO;
                    att.current_funnel_waypoints.clear();
                }
                continue;
            };

            let att_owner = self.entities.get(&attacker_id).map(|a| a.owner);
            let Some(att_owner) = att_owner else {
                continue;
            };

            if !self.is_entity_available_for_combat(att_owner, target_id) {
                if let Some(att) = self.entities.get_mut(&attacker_id) {
                    att.order_queue.pop_front();
                    att.velocity = SimVec2::ZERO;
                    att.current_funnel_waypoints.clear();
                }
                continue;
            }

            let target_arch = match self.catalog.get(target_type) {
                Ok(a) => a,
                Err(_) => continue,
            };

            let att = match self.entities.get(&attacker_id) {
                Some(a) => a,
                None => continue,
            };
            let Ok(att_arch) = self.catalog.get(att.unit_type_id) else {
                continue;
            };

            let active_weapon = att_arch.weapon_against(
                target_arch.mover_type,
                att.is_sieged.then_some(att.siege_weapon.as_ref()).flatten(),
            );
            let Some(weapon) = active_weapon else {
                if let Some(att_mut) = self.entities.get_mut(&attacker_id) {
                    att_mut.order_queue.pop_front();
                    att_mut.velocity = SimVec2::ZERO;
                    att_mut.current_funnel_waypoints.clear();
                }
                continue;
            };

            let speed = self.effective_max_speed(att_arch, att.owner) * att.speed_multiplier();
            let collision_radius = att_arch.collision_radius;
            let target_radius = target_arch.collision_radius;
            let has_turret = weapon.has_turret;
            let weapon_min = weapon.min_range;
            let weapon_max = self.effective_weapon_range(weapon, &att_arch.slug, att.owner);

            let dist = att.position.distance(target_pos);
            let effective_max = weapon_max + collision_radius + target_radius;
            let effective_min = if weapon_min > SimScalar::ZERO {
                weapon_min + collision_radius + target_radius
            } else {
                SimScalar::ZERO
            };

            let in_range = dist <= effective_max && dist >= effective_min;

            let att = match self.entities.get_mut(&attacker_id) {
                Some(a) => a,
                None => continue,
            };

            if in_range {
                att.velocity = SimVec2::ZERO;
                let delta = target_pos - att.position;
                let target_angle = SimAngle::atan2(delta.y, delta.x);

                if has_turret {
                    att.turret_angle = target_angle;
                } else {
                    att.heading = target_angle;
                }

                if matches!(
                    att.weapon_phase,
                    WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
                ) {
                    att.current_anim = AnimState::Attack;
                } else {
                    att.current_anim = AnimState::Idle;
                }
            } else if att.is_immobile {
                att.velocity = SimVec2::ZERO;
                att.current_anim = AnimState::Idle;
            } else {
                let steer_dir = compute_unified_local_steering(
                    attacker_id,
                    att.position,
                    collision_radius,
                    target_pos,
                    Some(target_id),
                    &obstacles,
                );
                let vel = steer_dir * speed;
                let next_pos = att.position + vel;

                if Self::is_position_pathable(&self.map, att.position, next_pos) {
                    att.velocity = vel;
                    att.position = next_pos;
                    att.heading = SimAngle::atan2(att.velocity.y, att.velocity.x);
                    att.current_anim = AnimState::Walk;
                } else {
                    let slide_x = SimVec2::new(next_pos.x, att.position.y);
                    if Self::is_position_pathable(&self.map, att.position, slide_x) {
                        att.velocity = SimVec2::new(vel.x, SimScalar::ZERO);
                        att.position = slide_x;
                        att.heading = SimAngle::atan2(att.velocity.y, att.velocity.x);
                        att.current_anim = AnimState::Walk;
                    } else {
                        let slide_y = SimVec2::new(att.position.x, next_pos.y);
                        if Self::is_position_pathable(&self.map, att.position, slide_y) {
                            att.velocity = SimVec2::new(SimScalar::ZERO, vel.y);
                            att.position = slide_y;
                            att.heading = SimAngle::atan2(att.velocity.y, att.velocity.x);
                            att.current_anim = AnimState::Walk;
                        } else {
                            att.velocity = SimVec2::ZERO;
                        }
                    }
                }
            }
        }
    }

    /// Checks whether `target_pos` is valid and passable for a ground unit currently at `current_pos`.
    #[must_use]
    #[inline]
    pub fn is_position_pathable_from(&self, current_pos: SimVec2, target_pos: SimVec2) -> bool {
        Self::is_position_pathable(&self.map, current_pos, target_pos)
    }

    /// Checks whether `target_pos` is valid and passable for a ground unit currently at `current_pos` on the given map.
    #[must_use]
    pub fn is_position_pathable(map: &SimMap, current_pos: SimVec2, target_pos: SimVec2) -> bool {
        let (Ok((c_x, c_y)), Ok((t_x, t_y))) = (
            map.world_to_cell(current_pos),
            map.world_to_cell(target_pos),
        ) else {
            return false;
        };

        let Ok(target_cell) = map.cell(t_x, t_y) else {
            return false;
        };

        // Cannot enter structures or unpassable cliffs
        if target_cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
            || target_cell.flags.contains(CellFlags::UNPASSABLE_CLIFF)
            || !target_cell.flags.contains(CellFlags::GROUND_PATHABLE)
        {
            return false;
        }

        // Cliff level transition check: must have ramp if cliff levels differ
        if c_x != t_x || c_y != t_y {
            if let Ok(cur_cell) = map.cell(c_x, c_y) {
                if cur_cell.cliff_level != target_cell.cliff_level
                    && cur_cell.ramp.is_none()
                    && target_cell.ramp.is_none()
                {
                    return false;
                }
            }
        }

        true
    }

    /// Phase 6: Resolve circle-circle collisions, engagement anchoring, and push priority.
    pub fn resolve_collisions_and_push(&mut self) {
        self.spatial_hash.clear();
        for (id, entity) in &self.entities {
            if !entity.is_loaded_in_transport {
                self.spatial_hash.insert(*id, entity.position);
            }
        }

        let entity_ids: Vec<EntityId> = self.entities.keys().copied().collect();
        let mut friendly_pushes: BTreeMap<EntityId, SimVec2> = BTreeMap::new();
        let mut hard_pushes: BTreeMap<EntityId, SimVec2> = BTreeMap::new();

        for &id_a in &entity_ids {
            let Some(entity_a) = self.entities.get(&id_a) else {
                continue;
            };
            if entity_a.is_mineral_walking || entity_a.is_loaded_in_transport {
                continue;
            }

            let radius_a = self
                .catalog
                .get(entity_a.unit_type_id)
                .map_or(SimScalar::HALF, |a| a.collision_radius);

            let pos_a = entity_a.position;
            let vel_a = entity_a.velocity;
            let owner_a = entity_a.owner;
            let is_holding_a = entity_a.is_holding_position;
            let priority_a = self
                .catalog
                .get(entity_a.unit_type_id)
                .map_or(0, |a| a.push_priority);

            let is_engaged_a = matches!(
                entity_a.weapon_phase,
                WeaponPhase::RotatingToTarget { .. }
                    | WeaponPhase::Windup { .. }
                    | WeaponPhase::Backswing { .. }
            ) || (entity_a.current_target.is_some()
                && entity_a.order_queue.front().is_some_and(|o| {
                    matches!(
                        o,
                        UnitOrder::AttackTarget { .. } | UnitOrder::AttackMove { .. }
                    )
                })
                && entity_a.velocity == SimVec2::ZERO);

            self.spatial_hash.query_radius(
                pos_a,
                radius_a * SimScalar::from_num(2),
                &mut self.scratch_neighbors,
            );

            for &id_b in &self.scratch_neighbors {
                if id_b.0 <= id_a.0 {
                    continue;
                }

                let Some(entity_b) = self.entities.get(&id_b) else {
                    continue;
                };
                if entity_b.is_mineral_walking || entity_b.is_loaded_in_transport {
                    continue;
                }

                let radius_b = self
                    .catalog
                    .get(entity_b.unit_type_id)
                    .map_or(SimScalar::HALF, |a| a.collision_radius);

                let combined = radius_a + radius_b;
                let dist = pos_a.distance(entity_b.position);

                if dist < combined {
                    let priority_b = self
                        .catalog
                        .get(entity_b.unit_type_id)
                        .map_or(0, |a| a.push_priority);

                    let is_engaged_b = matches!(
                        entity_b.weapon_phase,
                        WeaponPhase::RotatingToTarget { .. }
                            | WeaponPhase::Windup { .. }
                            | WeaponPhase::Backswing { .. }
                    ) || (entity_b.current_target.is_some()
                        && entity_b.order_queue.front().is_some_and(|o| {
                            matches!(
                                o,
                                UnitOrder::AttackTarget { .. } | UnitOrder::AttackMove { .. }
                            )
                        })
                        && entity_b.velocity == SimVec2::ZERO);

                    let eff_cannot_be_pushed_a = entity_a.cannot_be_pushed()
                        || (owner_a == entity_b.owner && is_engaged_a && priority_a >= priority_b);
                    let eff_cannot_be_pushed_b = entity_b.cannot_be_pushed()
                        || (owner_a == entity_b.owner && is_engaged_b && priority_b >= priority_a);

                    let a_params = PushEntityParams {
                        id: id_a,
                        pos: pos_a,
                        vel: vel_a,
                        owner: owner_a,
                        is_holding: is_holding_a,
                        push_priority: priority_a,
                        cannot_be_pushed: eff_cannot_be_pushed_a,
                    };
                    let b_params = PushEntityParams {
                        id: id_b,
                        pos: entity_b.position,
                        vel: entity_b.velocity,
                        owner: entity_b.owner,
                        is_holding: entity_b.is_holding_position,
                        push_priority: priority_b,
                        cannot_be_pushed: eff_cannot_be_pushed_b,
                    };
                    let (delta_a, delta_b) =
                        compute_push_displacement(a_params, b_params, dist, combined);

                    let is_hard_collision = owner_a != entity_b.owner;

                    if is_hard_collision {
                        if delta_a != SimVec2::ZERO {
                            *hard_pushes.entry(id_a).or_insert(SimVec2::ZERO) += delta_a;
                        }
                        if delta_b != SimVec2::ZERO {
                            *hard_pushes.entry(id_b).or_insert(SimVec2::ZERO) += delta_b;
                        }
                    } else {
                        if delta_a != SimVec2::ZERO {
                            *friendly_pushes.entry(id_a).or_insert(SimVec2::ZERO) += delta_a;
                        }
                        if delta_b != SimVec2::ZERO {
                            *friendly_pushes.entry(id_b).or_insert(SimVec2::ZERO) += delta_b;
                        }
                    }
                }
            }
        }

        // Apply soft friendly crowd pushes with Baumgarte relaxation and strict speed caps
        for (id, raw_push) in friendly_pushes {
            if let Some(ent) = self.entities.get_mut(&id) {
                let max_speed = self
                    .catalog
                    .get(ent.unit_type_id)
                    .map_or(SimScalar::ZERO, |a| a.max_speed);
                if max_speed <= SimScalar::ZERO {
                    continue;
                }

                let max_push_mag = (max_speed * SimScalar::from_f32_lossy(0.65))
                    .max(SimScalar::from_f32_lossy(0.015));
                let clamped_push = if raw_push.length_sq() > max_push_mag * max_push_mag {
                    raw_push.normalize_or_zero() * max_push_mag
                } else {
                    raw_push
                };

                let mut proposed_pos = ent.position + clamped_push;
                let total_tick_delta = proposed_pos - ent.pos_at_tick_start;
                if total_tick_delta.length_sq() > max_speed * max_speed {
                    proposed_pos =
                        ent.pos_at_tick_start + total_tick_delta.normalize_or_zero() * max_speed;
                }

                if Self::is_position_pathable(&self.map, ent.position, proposed_pos) {
                    ent.position = proposed_pos;
                } else {
                    let slide_x = SimVec2::new(proposed_pos.x, ent.position.y);
                    if Self::is_position_pathable(&self.map, ent.position, slide_x) {
                        ent.position = slide_x;
                    } else {
                        let slide_y = SimVec2::new(ent.position.x, proposed_pos.y);
                        if Self::is_position_pathable(&self.map, ent.position, slide_y) {
                            ent.position = slide_y;
                        }
                    }
                }
            }
        }

        // Apply hard obstacle collisions (enemy repulsion and immovable obstacles)
        for (id, hard_push) in hard_pushes {
            if let Some(ent) = self.entities.get_mut(&id) {
                let proposed_pos = ent.position + hard_push;
                if Self::is_position_pathable(&self.map, ent.position, proposed_pos) {
                    ent.position = proposed_pos;
                } else {
                    let slide_x = SimVec2::new(proposed_pos.x, ent.position.y);
                    if Self::is_position_pathable(&self.map, ent.position, slide_x) {
                        ent.position = slide_x;
                    } else {
                        let slide_y = SimVec2::new(ent.position.x, proposed_pos.y);
                        if Self::is_position_pathable(&self.map, ent.position, slide_y) {
                            ent.position = slide_y;
                        }
                    }
                }
            }
        }
    }

    /// Phase 7: Advance weapon state machines, fire attacks, and step projectiles.
    pub fn step_weapons_and_projectiles(&mut self) {
        // Collect target snapshots first to decouple reads from mutations
        let mut target_snapshots = BTreeMap::new();
        for (&id, entity) in &self.entities {
            let (radius, mover_type) = self
                .catalog
                .get(entity.unit_type_id)
                .map_or((SimScalar::HALF, sim_types::MoverType::Ground), |a| {
                    (a.collision_radius, a.mover_type)
                });
            target_snapshots.insert(id, (entity.position, radius, mover_type, entity.is_alive()));
        }

        let mut attacks_to_apply = Vec::new();

        for (id, entity) in &mut self.entities {
            if !entity.is_alive()
                || entity.channeling_remaining_ticks > 0
                || entity.is_loaded_in_transport
            {
                continue;
            }
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };

            let target_opt_id =
                if let Some(UnitOrder::AttackTarget { target }) = entity.order_queue.front() {
                    Some(*target)
                } else {
                    let has_turret = entity
                        .siege_weapon
                        .as_ref()
                        .or(arch.ground_weapon.as_ref())
                        .or(arch.weapon.as_ref())
                        .is_some_and(|w| w.has_turret);
                    if has_turret {
                        entity.current_target
                    } else {
                        None
                    }
                };

            let target_mover = target_opt_id
                .and_then(|tid| target_snapshots.get(&tid).map(|t| t.2))
                .unwrap_or(sim_types::MoverType::Ground);

            let siege_weapon_clone = entity
                .is_sieged
                .then(|| entity.siege_weapon.clone())
                .flatten();

            let active_weapon = arch
                .weapon_against(target_mover, siege_weapon_clone.as_ref())
                .cloned();
            let Some(weapon) = active_weapon else {
                continue;
            };

            let target_info = target_opt_id.and_then(|tid| {
                target_snapshots
                    .get(&tid)
                    .and_then(|&(pos, r, _, is_alive)| {
                        if is_alive {
                            Some((pos, r + arch.collision_radius))
                        } else {
                            None
                        }
                    })
            });

            let mut weapon = weapon;
            weapon.max_range = Self::effective_range_from_upgrades(
                &self.player_upgrades,
                &weapon,
                &arch.slug,
                entity.owner,
            );
            weapon.period_ticks = Self::effective_period_from_upgrades(
                &self.player_upgrades,
                &weapon,
                &arch.slug,
                entity.owner,
            );

            let fired = step_entity_weapon(entity, &weapon, target_info);
            if fired {
                if let Some(target) = target_opt_id {
                    attacks_to_apply.push((*id, target, weapon, arch.slug.clone()));
                }
            }
        }

        // Apply attacks
        for (attacker_id, target_id, weapon, attacker_slug) in attacks_to_apply {
            let attacker_pos = self
                .entities
                .get(&attacker_id)
                .map_or(SimVec2::ZERO, |a| a.position);
            let attacker_owner = self
                .entities
                .get(&attacker_id)
                .map_or(PlayerId(0), |a| a.owner);

            if let Some(proj_speed) = weapon.projectile_speed {
                let target_pos = self
                    .entities
                    .get(&target_id)
                    .map_or(attacker_pos, |t| t.position);
                let p_id = self.next_projectile_id;
                self.next_projectile_id = self.next_projectile_id.wrapping_add(1);

                let attacker_radius = self
                    .catalog
                    .get(
                        self.entities
                            .get(&attacker_id)
                            .map_or(UnitTypeId(0), |a| a.unit_type_id),
                    )
                    .map_or(SimScalar::HALF, |a| a.collision_radius);
                let target_radius = self
                    .entities
                    .get(&target_id)
                    .and_then(|t| self.catalog.get(t.unit_type_id).ok())
                    .map_or(SimScalar::HALF, |a| a.collision_radius);

                let delta = target_pos - attacker_pos;
                let dist = delta.length();
                let dir = delta.normalize_or_zero();

                let eff_dist =
                    (dist - target_radius - attacker_radius).max(SimScalar::from_num(0.05));
                let ticks_to_impact = (eff_dist / proj_speed).0.ceil().to_num::<u32>().max(1);
                let flight_duration_secs = (ticks_to_impact as f32) / 25.0;

                self.projectiles.push(ActiveProjectile {
                    id: p_id,
                    owner: attacker_owner,
                    attacker_id: Some(attacker_id),
                    source_pos: attacker_pos,
                    current_pos: attacker_pos,
                    target_id: Some(target_id),
                    target_pos,
                    speed: proj_speed,
                    weapon_spec: weapon.clone(),
                    source_slug: Some(attacker_slug),
                    remaining_lifetime_ticks: weapon.max_projectile_lifetime_ticks,
                });

                // Emit visual projectile event for client VFX rendering
                let spawn_pos_2d = attacker_pos + dir * attacker_radius;
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
                        team_id: TeamId(attacker_owner.0),
                        duration_secs: Some(flight_duration_secs),
                    });
            } else {
                // Instant hitscan / beam / melee
                let target_data = self
                    .entities
                    .get(&target_id)
                    .map(|t| (t.position, t.shields, t.hp, t.unit_type_id, t.owner));

                if let Some((target_pos, shields, hp, unit_type, target_owner)) = target_data {
                    if let Ok(target_arch) = self.catalog.get(unit_type) {
                        let hit_angle = SimAngle::atan2(
                            attacker_pos.y - target_pos.y,
                            attacker_pos.x - target_pos.x,
                        );

                        let mut cur_shields = shields;
                        let mut cur_hp = hp;
                        let attacker_upgrades =
                            self.player_upgrades[(attacker_owner.0 as usize).min(7)].clone();
                        let target_upgrades =
                            self.player_upgrades[(target_owner.0 as usize).min(7)].clone();

                        for _ in 0..weapon.attacks_count {
                            let res = calculate_hit_damage_upgraded(
                                &weapon,
                                Some(&attacker_slug),
                                &attacker_upgrades,
                                target_arch,
                                &target_upgrades,
                                cur_shields,
                                cur_hp,
                            );
                            cur_shields = (cur_shields - res.shield_damage).max(SimScalar::ZERO);
                            cur_hp = (cur_hp - res.life_damage).max(SimScalar::ZERO);

                            if let Some(target_ent) = self.entities.get_mut(&target_id) {
                                target_ent.apply_damage(
                                    res.shield_damage,
                                    res.life_damage,
                                    hit_angle,
                                );
                            }
                        }

                        // Apply combat retaliation aggro
                        self.apply_combat_retaliation(target_id, attacker_id);

                        // Apply circular splash if defined for hitscan/instant attack
                        if let Some(splash) = &weapon.splash {
                            let mut splash_hits = Vec::new();
                            for (&other_id, other) in &self.entities {
                                if other_id == target_id
                                    || other_id == attacker_id
                                    || !other.is_alive()
                                {
                                    continue;
                                }
                                if !splash.friendly_fire && other.owner == attacker_owner {
                                    continue;
                                }
                                let other_dist = target_pos.distance(other.position);
                                if other_dist <= splash.outer_radius {
                                    if let Ok(other_arch) = self.catalog.get(other.unit_type_id) {
                                        let other_upgrades = self.player_upgrades
                                            [(other.owner.0 as usize).min(7)]
                                        .clone();
                                        if let Some(splash_res) = calculate_splash_damage_upgraded(
                                            &weapon,
                                            Some(&attacker_slug),
                                            &attacker_upgrades,
                                            splash,
                                            other_dist,
                                            other_arch,
                                            &other_upgrades,
                                            other.shields,
                                            other.hp,
                                        ) {
                                            splash_hits.push((other_id, splash_res, hit_angle));
                                        }
                                    }
                                }
                            }
                            for (spl_id, spl_res, spl_angle) in splash_hits {
                                if let Some(spl_ent) = self.entities.get_mut(&spl_id) {
                                    spl_ent.apply_damage(
                                        spl_res.shield_damage,
                                        spl_res.life_damage,
                                        spl_angle,
                                    );
                                }
                            }
                        }

                        // Record visual event
                        let z = self.map.terrain_altitude_at(attacker_pos);
                        let tz = self.map.terrain_altitude_at(target_pos);
                        match weapon.visual_kind {
                            WeaponVisualKind::Hitscan => {
                                self.pending_events.push(CombatEventVisual::HitscanTracer {
                                    from: [
                                        attacker_pos.x.to_f32(),
                                        attacker_pos.y.to_f32(),
                                        z + 0.5,
                                    ],
                                    to: [target_pos.x.to_f32(), target_pos.y.to_f32(), tz + 0.5],
                                    team_id: TeamId(attacker_owner.0),
                                });
                            }
                            WeaponVisualKind::LaserBeam => {
                                self.pending_events.push(CombatEventVisual::BeamVisual {
                                    from: [
                                        attacker_pos.x.to_f32(),
                                        attacker_pos.y.to_f32(),
                                        z + 0.5,
                                    ],
                                    to: [target_pos.x.to_f32(), target_pos.y.to_f32(), tz + 0.5],
                                    is_lightning: false,
                                    team_id: TeamId(attacker_owner.0),
                                });
                            }
                            WeaponVisualKind::LightningArc => {
                                self.pending_events.push(CombatEventVisual::BeamVisual {
                                    from: [
                                        attacker_pos.x.to_f32(),
                                        attacker_pos.y.to_f32(),
                                        z + 0.5,
                                    ],
                                    to: [target_pos.x.to_f32(), target_pos.y.to_f32(), tz + 0.5],
                                    is_lightning: true,
                                    team_id: TeamId(attacker_owner.0),
                                });
                            }
                            WeaponVisualKind::MeleeSlash => {
                                self.pending_events
                                    .push(CombatEventVisual::MeleeSlashVisual {
                                        pos: [
                                            target_pos.x.to_f32(),
                                            target_pos.y.to_f32(),
                                            tz + 0.5,
                                        ],
                                        angle_rad: hit_angle.to_radians_f32(),
                                        radius: 0.5,
                                        team_id: TeamId(attacker_owner.0),
                                    });
                            }
                            _ => {}
                        }

                        let _ = target_owner;
                    }
                }
            }
        }

        // Step active projectiles and seeking ordnance
        let mut completed_projectiles = Vec::new();
        for (idx, proj) in self.projectiles.iter_mut().enumerate() {
            // 1. Timeout Expiration Check
            proj.remaining_lifetime_ticks = proj.remaining_lifetime_ticks.saturating_sub(1);
            if proj.remaining_lifetime_ticks == 0 {
                // Fueled out / timed out before reaching target
                completed_projectiles.push((idx, true));
                continue;
            }

            let mut target_radius = SimScalar::HALF;

            // 2. Target Availability & Retargeting Check
            if let Some(target_id) = proj.target_id {
                if Self::is_target_available_for_combat(
                    &self.entities,
                    &self.map,
                    &self.vision_maps,
                    proj.owner,
                    target_id,
                ) {
                    if let Some(target) = self.entities.get(&target_id) {
                        proj.target_pos = target.position;
                        target_radius = self
                            .catalog
                            .get(target.unit_type_id)
                            .map_or(SimScalar::HALF, |a| a.collision_radius);
                    }
                } else {
                    // Target became unavailable (cargo loaded, cloaked without detector, invulnerable, died, or expired)
                    match proj.weapon_spec.target_loss_behavior {
                        ProjectileTargetLossBehavior::Fizzle => {
                            // Fizzle immediately with 0 damage
                            completed_projectiles.push((idx, true));
                            continue;
                        }
                        ProjectileTargetLossBehavior::ImpactLastKnownPosition => {
                            // Clear target_id, keep target_pos as ballistic coordinate
                            proj.target_id = None;
                            target_radius = SimScalar::ZERO;
                        }
                        ProjectileTargetLossBehavior::RetargetNearest { search_radius } => {
                            // Scan spatial hash for nearest available enemy matching weapon target_flags
                            let mut best_cand: Option<(EntityId, SimVec2, SimScalar, SimScalar)> =
                                None;
                            let search_r2 = search_radius * search_radius;

                            self.scratch_neighbors.clear();
                            self.spatial_hash.query_radius(
                                proj.current_pos,
                                search_radius,
                                &mut self.scratch_neighbors,
                            );
                            for &cand_id in &self.scratch_neighbors {
                                if Some(cand_id) == proj.target_id
                                    || Some(cand_id) == proj.attacker_id
                                {
                                    continue;
                                }
                                if let Some(cand) = self.entities.get(&cand_id) {
                                    if cand.owner == proj.owner {
                                        continue;
                                    }
                                    if !Self::is_target_available_for_combat(
                                        &self.entities,
                                        &self.map,
                                        &self.vision_maps,
                                        proj.owner,
                                        cand_id,
                                    ) {
                                        continue;
                                    }
                                    if let Ok(cand_arch) = self.catalog.get(cand.unit_type_id) {
                                        if !proj
                                            .weapon_spec
                                            .target_flags
                                            .intersects(cand_arch.attributes)
                                        {
                                            continue;
                                        }
                                        let d2 = proj.current_pos.distance_sq(cand.position);
                                        if d2 <= search_r2 {
                                            let is_better =
                                                best_cand.as_ref().map_or(true, |b| d2 < b.3);
                                            if is_better {
                                                best_cand = Some((
                                                    cand_id,
                                                    cand.position,
                                                    cand_arch.collision_radius,
                                                    d2,
                                                ));
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some((new_id, new_pos, new_radius, _)) = best_cand {
                                proj.target_id = Some(new_id);
                                proj.target_pos = new_pos;
                                target_radius = new_radius;
                            } else {
                                // No alternate target found within search radius -> fizzle
                                completed_projectiles.push((idx, true));
                                continue;
                            }
                        }
                    }
                }
            } else {
                target_radius = SimScalar::ZERO;
            }

            // 3. Advance projectile kinematics
            let delta = proj.target_pos - proj.current_pos;
            let dist = delta.length();

            // 4. Surface Impact Check
            if dist <= proj.speed + target_radius {
                completed_projectiles.push((idx, false));
            } else {
                proj.current_pos += delta.normalize_or_zero() * proj.speed;
            }
        }

        for (idx, disjointed) in completed_projectiles.into_iter().rev() {
            let proj = self.projectiles.remove(idx);
            if disjointed {
                continue;
            }

            // Apply impact damage
            if let Some(target_id) = proj.target_id {
                let target_data = self
                    .entities
                    .get(&target_id)
                    .map(|t| (t.position, t.shields, t.hp, t.unit_type_id, t.owner));

                if let Some((target_pos, shields, hp, unit_type, target_owner)) = target_data {
                    if let Ok(target_arch) = self.catalog.get(unit_type) {
                        let hit_angle = SimAngle::atan2(
                            proj.source_pos.y - target_pos.y,
                            proj.source_pos.x - target_pos.x,
                        );

                        let attacker_upgrades =
                            self.player_upgrades[(proj.owner.0 as usize).min(7)].clone();
                        let target_upgrades =
                            self.player_upgrades[(target_owner.0 as usize).min(7)].clone();

                        let res = calculate_hit_damage_upgraded(
                            &proj.weapon_spec,
                            proj.source_slug.as_deref(),
                            &attacker_upgrades,
                            target_arch,
                            &target_upgrades,
                            shields,
                            hp,
                        );

                        if let Some(target_ent) = self.entities.get_mut(&target_id) {
                            target_ent.apply_damage(res.shield_damage, res.life_damage, hit_angle);
                        }

                        if let Some(attacker_id) = proj.attacker_id {
                            self.apply_combat_retaliation(target_id, attacker_id);
                        }

                        // Apply circular splash if defined
                        if let Some(splash) = &proj.weapon_spec.splash {
                            let mut splash_hits = Vec::new();
                            for (&other_id, other) in &self.entities {
                                if other_id == target_id || !other.is_alive() {
                                    continue;
                                }
                                if !splash.friendly_fire && other.owner == proj.owner {
                                    continue;
                                }
                                let other_dist = target_pos.distance(other.position);
                                if other_dist <= splash.outer_radius {
                                    if let Ok(other_arch) = self.catalog.get(other.unit_type_id) {
                                        let other_upgrades = self.player_upgrades
                                            [(other.owner.0 as usize).min(7)]
                                        .clone();
                                        if let Some(splash_res) = calculate_splash_damage_upgraded(
                                            &proj.weapon_spec,
                                            proj.source_slug.as_deref(),
                                            &attacker_upgrades,
                                            splash,
                                            other_dist,
                                            other_arch,
                                            &other_upgrades,
                                            other.shields,
                                            other.hp,
                                        ) {
                                            splash_hits.push((other_id, splash_res, hit_angle));
                                        }
                                    }
                                }
                            }
                            for (spl_id, spl_res, spl_angle) in splash_hits {
                                if let Some(spl_ent) = self.entities.get_mut(&spl_id) {
                                    spl_ent.apply_damage(
                                        spl_res.shield_damage,
                                        spl_res.life_damage,
                                        spl_angle,
                                    );
                                }
                            }
                        }
                    }
                }
            } else if let Some(splash) = &proj.weapon_spec.splash {
                // Ballistic impact at ground coordinate (ImpactLastKnownPosition) with splash
                let target_pos = proj.target_pos;
                let hit_angle = SimAngle::atan2(
                    proj.source_pos.y - target_pos.y,
                    proj.source_pos.x - target_pos.x,
                );
                let attacker_upgrades =
                    self.player_upgrades[(proj.owner.0 as usize).min(7)].clone();

                let mut splash_hits = Vec::new();
                for (&other_id, other) in &self.entities {
                    if !other.is_alive() {
                        continue;
                    }
                    if !splash.friendly_fire && other.owner == proj.owner {
                        continue;
                    }
                    let other_dist = target_pos.distance(other.position);
                    if other_dist <= splash.outer_radius {
                        if let Ok(other_arch) = self.catalog.get(other.unit_type_id) {
                            let other_upgrades =
                                self.player_upgrades[(other.owner.0 as usize).min(7)].clone();
                            if let Some(splash_res) = calculate_splash_damage_upgraded(
                                &proj.weapon_spec,
                                proj.source_slug.as_deref(),
                                &attacker_upgrades,
                                splash,
                                other_dist,
                                other_arch,
                                &other_upgrades,
                                other.shields,
                                other.hp,
                            ) {
                                splash_hits.push((other_id, splash_res, hit_angle));
                            }
                        }
                    }
                }
                for (spl_id, spl_res, spl_angle) in splash_hits {
                    if let Some(spl_ent) = self.entities.get_mut(&spl_id) {
                        spl_ent.apply_damage(spl_res.shield_damage, spl_res.life_damage, spl_angle);
                    }
                }
            }
        }
    }

    /// Phase 8: Reap destroyed entities and emit death shatter visuals.
    pub fn reap_dead_and_emit_shatter(&mut self) {
        self.scratch_dead_entities.clear();

        for (&id, entity) in &self.entities {
            if !entity.is_alive() {
                self.scratch_dead_entities.push(id);
            }
        }

        for id in &self.scratch_dead_entities {
            if let Some(dead) = self.entities.remove(id) {
                // If building, remove footprint from map
                let _ = self.map.remove_structure(*id);
                self.pathfinder.recompute_clearance(&self.map);

                // Emit shatter visual
                let z = self.map.terrain_altitude_at(dead.position);
                let (radius, is_dome) =
                    self.catalog
                        .get(dead.unit_type_id)
                        .map_or((0.5, false), |a| {
                            (
                                a.collision_radius.to_f32(),
                                matches!(a.shape, ShapeKind::BuildingDome { .. }),
                            )
                        });

                self.pending_events.push(CombatEventVisual::BubbleShatter {
                    pos: [dead.position.x.to_f32(), dead.position.y.to_f32(), z],
                    radius,
                    is_dome,
                    team_id: TeamId(dead.owner.0),
                });
            }
        }
    }

    /// Phase 9: Compute authoritative 64-bit state checksum.
    #[must_use]
    pub fn compute_checksum(&self) -> u64 {
        compute_world_checksum(
            u64::from(self.current_tick),
            self.rng_seed,
            &self.entities,
            &self.projectiles,
        )
    }

    /// Phase 10: Build immutable render snapshot for client visualization.
    pub fn build_render_snapshot(&mut self) -> RenderSnapshot {
        let mut bubbles = Vec::with_capacity(self.entities.len());

        for entity in self.entities.values() {
            let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
                continue;
            };

            let ground_z = self.map.terrain_altitude_at(entity.position);
            let altitude_z = if matches!(arch.mover_type, sim_types::MoverType::Air) {
                ground_z + 2.5
            } else {
                ground_z
            };

            let is_attacking = matches!(
                entity.weapon_phase,
                WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
            );

            let hp_ratio = if arch.max_hp > SimScalar::ZERO {
                (entity.hp / arch.max_hp).to_f32().clamp(0.0, 1.0)
            } else {
                0.0
            };

            let shield_ratio = if arch.max_shields > SimScalar::ZERO {
                (entity.shields / arch.max_shields).to_f32().clamp(0.0, 1.0)
            } else {
                0.0
            };

            let shield_flash = (f32::from(entity.shield_hit_flash_ticks)) / 8.0;
            let hull_flash = (f32::from(entity.hull_hit_flash_ticks)) / 8.0;

            let speed_boosted = entity.active_buffs.iter().any(|b| b.buff_id == 1);

            bubbles.push(BubbleRenderInstance {
                entity_id: entity.id,
                shape: arch.shape,
                silhouette: arch.silhouette,
                unit_type: entity.unit_type_id,
                team_id: TeamId(entity.owner.0),
                world_pos: [
                    entity.position.x.to_f32(),
                    entity.position.y.to_f32(),
                    altitude_z,
                ],
                ground_z,
                radius: arch.collision_radius.to_f32(),
                velocity: [entity.velocity.x.to_f32(), entity.velocity.y.to_f32()],
                heading_angle_rad: entity.heading.to_radians_f32(),
                aim_angle_rad: entity.turret_angle.to_radians_f32(),
                is_attacking,
                weapon_visual: entity
                    .siege_weapon
                    .as_ref()
                    .map(|w| w.visual_kind)
                    .or_else(|| arch.ground_weapon.as_ref().map(|w| w.visual_kind))
                    .or_else(|| arch.weapon.as_ref().map(|w| w.visual_kind))
                    .unwrap_or(WeaponVisualKind::None),
                attack_target_pos: None,
                anim_state: entity.current_anim,
                anim_progress: entity.anim_progress.to_f32(),
                hp_ratio,
                shield_ratio,
                shield_hit_flash: shield_flash,
                hull_hit_angle_rad: entity.last_hit_angle.to_radians_f32(),
                hull_hit_flash: hull_flash,
                build_or_queue_progress: None,
                selected: false,
                cloaked: entity.is_cloaked,
                speed_boosted,
            });
        }

        let combat_events = std::mem::take(&mut self.pending_events);

        RenderSnapshot {
            tick: Tick(self.current_tick),
            checksum: self.last_checksum,
            bubbles,
            combat_events,
        }
    }
}
