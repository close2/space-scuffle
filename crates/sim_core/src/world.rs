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
use crate::checksum::compute_simulation_checksum;
use crate::combat::{
    calculate_hit_damage_upgraded, calculate_splash_damage_upgraded, compute_threat_priority,
    step_entity_weapon, TargetPriorityKey,
};
use crate::entity::{ActiveProjectile, Entity, WeaponPhase};
use crate::error::{SimCoreError, SimResult};
use crate::movement::{
    DisplacementAdmissionError, MotionPolicy, MovementInvariant, MovementLayer, MovementMathError,
    MovementOutcome,
};
use crate::steering::{
    compute_push_displacement_with_mobility, CollisionMobility, PushEntityParams, SpatialHashGrid,
    SPATIAL_CELL_SIZE,
};
use crate::upgrades::UpgradeCatalog;
use crate::weapon_timing::{resolve_attack_period, AttackRate, SimulationDiagnostic};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, Pathfinder, PlayerVisionMap, SimMap, VisionSource,
};
use sim_types::{
    AbilityEffectSpec, AttributeFlags, BubbleRenderInstance, CombatEventVisual, EntityId, PlayerId,
    PlayerUpgradeState, ProjectileTargetLossBehavior, RenderSnapshot, ShapeKind, SimAngle,
    SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId, VisibilityState, WeaponVisualKind,
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
    /// Registered entity supply plus all queued reservations; death releases on reap.
    pub supply_used: u32,
    /// Effective supply capacity from completed providers, capped at 200.
    pub supply_max: u32,
    /// Uncapped completed-provider total; effective capacity is capped at 200.
    pub supply_provided_total: u32,
}

impl Default for PlayerResources {
    fn default() -> Self {
        Self {
            minerals: 1000,
            gas: 500,
            supply_used: 0,
            supply_max: 0,
            supply_provided_total: 0,
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
    /// Derived diagnostic output from the latest tick; excluded from state checksums.
    /// Consumers must surface these failures instead of treating the tick as healthy.
    pub(crate) tick_diagnostics: Vec<SimulationDiagnostic>,
    pub(crate) movement: crate::movement::MovementWorkspace,
    pub(crate) transport: crate::transport::TransportWorkspace,
    #[cfg(test)]
    pub(crate) transport_validation_count: std::cell::Cell<usize>,
    pub(crate) production: crate::production::ProductionWorkspace,
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
}

impl SimWorld {
    /// Constructs a world using the embedded compatibility catalogs and deterministic seed.
    ///
    /// Normal external-asset startup uses [`Self::with_catalogs`] instead.
    #[must_use]
    pub fn new(map: SimMap, seed: u64) -> Self {
        Self::with_catalogs(
            map,
            seed,
            UnitCatalog::new_starter_roster(),
            UpgradeCatalog::default_catalog(),
        )
    }

    /// Constructs a world on `map` using the supplied rule catalogs and RNG seed.
    ///
    /// Takes ownership of already loaded catalogs without reading files or replacing
    /// them with embedded defaults. Coordinates remain fixed-point world units.
    #[must_use]
    pub fn with_catalogs(
        map: SimMap,
        seed: u64,
        catalog: UnitCatalog,
        upgrade_catalog: UpgradeCatalog,
    ) -> Self {
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
            upgrade_catalog,
            player_upgrades: Default::default(),
            spatial_hash,
            player_resources: [PlayerResources::default(); 8],
            last_checksum: 0,
            pending_events: Vec::new(),
            tick_diagnostics: Vec::new(),
            movement: crate::movement::MovementWorkspace::default(),
            transport: crate::transport::TransportWorkspace::default(),
            #[cfg(test)]
            transport_validation_count: std::cell::Cell::new(0),
            production: crate::production::ProductionWorkspace::new(width, height),
            active_hazards: Vec::new(),
            next_hazard_id: 1,
            vision_aware_pathfinding: true,
            known_blocked_cells,
            scratch_neighbors: Vec::with_capacity(32),
            scratch_targets: Vec::with_capacity(64),
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
    /// Returns typed catalog, owner, vitals, allocation or captured-supply failures.
    /// Setup may begin over capacity; player training admission may not.
    pub fn spawn_unit(
        &mut self,
        unit_type: UnitTypeId,
        owner: PlayerId,
        pos: SimVec2,
    ) -> SimResult<EntityId> {
        self.validate_supply_ledger()?;
        let prepared = self.prepare_spawn(unit_type, owner, pos, 0)?;
        if matches!(prepared.entity_shape, ShapeKind::BuildingDome { .. }) {
            return Err(crate::production::ProductionInvariant::InvalidDefinition {
                unit_type,
                field: "mobile spawn requires a mobile shape",
            }
            .into());
        }
        Ok(self.commit_spawn(prepared))
    }

    /// Atomically spawns a completed setup structure at minimum grid cell `(cell_x, cell_y)`.
    /// Setup does not charge resources, but captures supply and never consumes an ID on failure.
    ///
    /// # Errors
    /// Returns typed identity, owner, vitals, allocator, supply or placement failures.
    pub fn spawn_structure(
        &mut self,
        structure_type: UnitTypeId,
        owner: PlayerId,
        cell_x: u32,
        cell_y: u32,
    ) -> SimResult<EntityId> {
        self.validate_supply_ledger()?;
        let archetype = self.catalog.get(structure_type)?;
        let ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
        } = archetype.shape
        else {
            return Err(crate::production::ProductionInvariant::InvalidDefinition {
                unit_type: structure_type,
                field: "structure spawn requires a dome shape",
            }
            .into());
        };
        if footprint_w == 0
            || footprint_h == 0
            || u64::from(cell_x) + u64::from(footprint_w) > u64::from(self.map.width())
            || u64::from(cell_y) + u64::from(footprint_h) > u64::from(self.map.height())
        {
            return Err(SimCoreError::InvalidBuildingPlacement {
                x: cell_x,
                y: cell_y,
            });
        }
        let pos = SimVec2::new(
            SimScalar::from_num(cell_x) + SimScalar::from_num(footprint_w) * SimScalar::HALF,
            SimScalar::from_num(cell_y) + SimScalar::from_num(footprint_h) * SimScalar::HALF,
        );
        let prepared = self.prepare_spawn(structure_type, owner, pos, 0)?;
        self.map
            .place_structure(prepared.entity.id, cell_x, cell_y, footprint_w, footprint_h)
            .map_err(|_| SimCoreError::InvalidBuildingPlacement {
                x: cell_x,
                y: cell_y,
            })?;
        let id = self.commit_spawn(prepared);
        self.pathfinder.recompute_clearance(&self.map);
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
    /// Returns typed errors for unknown research or unrepresentable cumulative/live stats.
    /// On failure all player state, resources and entity vitals remain unchanged.
    pub fn research_upgrade(&mut self, player_id: PlayerId, upgrade_slug: &str) -> SimResult<()> {
        let upgrade = self
            .upgrade_catalog
            .get_by_slug(upgrade_slug)
            .ok_or_else(|| SimCoreError::UnknownUpgrade(upgrade_slug.to_string()))?
            .clone();

        let player_index = usize::from(player_id.0);
        let mut staged_player = self
            .player_upgrades
            .get(player_index)
            .ok_or(MovementMathError::InvalidOwner(player_id))?
            .clone();
        staged_player.apply_upgrade(upgrade_slug, &upgrade.effects)?;
        for archetype in self.catalog.archetypes() {
            Self::effective_speed_with_state(archetype, &staged_player)?;
        }

        // Stage live vitals as well as research state: a late overflow must not
        // leave earlier entities, completed slugs or other effects committed.
        let mut staged_vitals = Vec::new();
        for (&id, entity) in &mut self.entities {
            if entity.owner != player_id {
                continue;
            }
            let archetype = self.catalog.get(entity.unit_type_id)?;
            Self::effective_speed_with_state(archetype, &staged_player)?
                .checked_mul(entity.speed_multiplier()?)
                .ok_or(MovementMathError::Unrepresentable)?;
            let mut hp = entity.hp;
            let mut max_hp = entity.max_hp;
            for effect in &upgrade.effects {
                if let sim_types::UpgradeEffect::ModifyUnitStat {
                    target_slug,
                    max_hp_delta,
                    ..
                } = effect
                {
                    if archetype.slug == *target_slug && *max_hp_delta > SimScalar::ZERO {
                        let overflow = || SimCoreError::UpgradeEntityVitalsOverflow { entity: id };
                        hp = hp.checked_add(*max_hp_delta).ok_or_else(overflow)?;
                        max_hp = max_hp.checked_add(*max_hp_delta).ok_or_else(overflow)?;
                    }
                }
            }
            if hp != entity.hp || max_hp != entity.max_hp {
                staged_vitals.push((entity, hp, max_hp));
            }
        }

        self.player_upgrades[player_index] = staged_player;
        for (entity, hp, max_hp) in staged_vitals {
            entity.hp = hp;
            entity.max_hp = max_hp;
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

    pub(crate) fn effective_speed_from_upgrades(
        upgrades: &[PlayerUpgradeState; 8],
        archetype: &UnitArchetype,
        owner: PlayerId,
    ) -> Result<SimScalar, MovementMathError> {
        let state = upgrades
            .get(usize::from(owner.0))
            .ok_or(MovementMathError::InvalidOwner(owner))?;
        Self::effective_speed_with_state(archetype, state)
    }

    fn effective_speed_with_state(
        archetype: &UnitArchetype,
        state: &PlayerUpgradeState,
    ) -> Result<SimScalar, MovementMathError> {
        if archetype.max_speed < SimScalar::ZERO {
            return Err(MovementMathError::NegativeSpeed);
        }
        let delta = state
            .unit_stat_modifiers
            .get(&archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.move_speed_delta);
        let speed = archetype
            .max_speed
            .checked_add(delta)
            .ok_or(MovementMathError::Unrepresentable)?;
        if speed < SimScalar::ZERO {
            return Err(MovementMathError::NegativeSpeed);
        }
        Ok(speed)
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
        resolve_attack_period(weapon.period_ticks, cd_delta, AttackRate::ONE)
    }

    /// Computes nonnegative movement speed in world units per25Hz tick, including upgrades.
    ///
    /// # Errors
    /// Rejects an invalid owner, negative authored/effective speed or numeric overflow.
    pub fn effective_max_speed(
        &self,
        archetype: &UnitArchetype,
        owner: PlayerId,
    ) -> Result<SimScalar, MovementMathError> {
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

    /// Computes the upgraded interval in 25 Hz ticks, clamped to `1..=u16::MAX`.
    /// Temporary rate buffs are excluded from this upgrade-only query.
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
    /// Consumers must inspect [`Self::diagnostics`] afterward and report any failures
    /// before continuing the match. Invalid weapon rates suppress the affected attack
    /// and produce typed evidence; they are never silently replaced by a default rate.
    pub fn step(&mut self, input: &TickBundle) -> RenderSnapshot {
        let mut output = RenderSnapshot::default();
        self.step_into(input, &mut output);
        output
    }

    /// Advances one authoritative tick into reusable client-owned output storage.
    ///
    /// `input.tick` retains the existing pre-step boundary convention; the output
    /// identifies the incremented, fully executed tick. Inspect diagnostics and
    /// drain command outcomes after each call, before the next tick clears them.
    /// Previous output contents are replaced, and pending visual events drain once.
    pub fn step_into(&mut self, input: &TickBundle, output: &mut RenderSnapshot) {
        self.tick_diagnostics.clear();
        self.production.outcomes.clear();
        self.movement.outcomes.clear();
        self.current_tick = self.current_tick.wrapping_add(1);

        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            self.last_checksum = self.compute_checksum();
            self.write_render_snapshot(output);
            return;
        }
        self.process_commands_prevalidated(input);
        self.update_production_and_buffs();
        self.update_vision();
        self.acquire_targets();
        self.step_movement_and_steering();
        self.resolve_collisions_and_push();
        self.step_weapons_and_projectiles();
        self.reap_dead_and_emit_shatter();
        self.last_checksum = self.compute_checksum();
        self.write_render_snapshot(output);
    }

    /// Returns explicit failures observed in the latest tick without consuming them.
    /// Direct phase calls append diagnostics until drained or the next full tick begins.
    #[must_use]
    pub fn diagnostics(&self) -> &[SimulationDiagnostic] {
        &self.tick_diagnostics
    }

    /// Consumes diagnostic evidence while retaining the output buffer allocation.
    /// Normal clients must report any returned error before continuing the match.
    pub fn drain_diagnostics(&mut self) -> std::vec::Drain<'_, SimulationDiagnostic> {
        self.tick_diagnostics.drain(..)
    }

    /// Returns the most recently computed 64-bit state checksum.
    #[must_use]
    #[inline]
    pub fn last_checksum(&self) -> u64 {
        self.last_checksum
    }

    /// Executes an ability, surfacing malformed numeric state through tick diagnostics.
    /// Ordinary eligibility/placement rejection returns false without payment.
    pub fn execute_ability(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            return false;
        }
        self.execute_ability_prevalidated(entity_id, ability_index, target_pos, target_entity)
    }

    // Tick/standalone wrapper proved relations; effect admission cannot alter links.
    fn execute_ability_prevalidated(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> bool {
        match self.try_execute_ability_prevalidated(
            entity_id,
            ability_index,
            target_pos,
            target_entity,
        ) {
            Ok(applied) => applied,
            Err(source) => {
                self.report_movement_invariant(source);
                false
            }
        }
    }

    fn apply_world_overclock(
        &mut self,
        id: EntityId,
        spec: sim_types::OverclockSpec,
    ) -> Result<bool, MovementInvariant> {
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "Overclock actor disappeared",
            })?;
        if actor.is_loaded_in_transport() {
            return Ok(false);
        }
        let prepared = actor
            .prepare_overclock(spec)
            .map_err(|source| MovementInvariant::Admission { entity: id, source })?;
        let Some(prepared) = prepared else {
            return Ok(false);
        };
        let arch = self
            .catalog
            .get(actor.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(id))?;
        self.effective_max_speed(arch, actor.owner)
            .and_then(|speed| {
                speed
                    .checked_mul(prepared.multiplier)
                    .ok_or(MovementMathError::Unrepresentable)
            })
            .map_err(|source| MovementInvariant::Arithmetic {
                entity: id,
                operation: "Overclock effective speed",
                source,
            })?;
        if let Some(actor) = self.entities.get_mut(&id) {
            actor.commit_overclock(prepared);
        }
        Ok(true)
    }

    /// Executes an active ability or spell for an entity.
    ///
    /// Validates energy cost, cooldowns, range, terrain pathability, and channeling.
    /// Returns `true` if cast successfully; `false` otherwise.
    pub(crate) fn try_execute_ability_prevalidated(
        &mut self,
        entity_id: EntityId,
        ability_index: u8,
        target_pos: Option<SimVec2>,
        target_entity: Option<EntityId>,
    ) -> Result<bool, MovementInvariant> {
        let _ = target_entity;
        let entity = match self.entities.get(&entity_id) {
            Some(e) if e.is_alive() => e,
            _ => return Ok(false),
        };

        if entity.is_loaded_in_transport() || entity.channeling_remaining_ticks > 0 {
            return Ok(false);
        }

        let arch = self
            .catalog
            .get(entity.unit_type_id)
            .map_err(|_| MovementInvariant::MissingArchetype(entity_id))?;

        let ability = match arch.abilities.get(ability_index as usize) {
            Some(a) => a,
            None => return Ok(false),
        };

        if let Some(&cd) = entity.ability_cooldowns.get(ability_index as usize) {
            if cd > 0 {
                return Ok(false);
            }
        }

        let admission = |source| MovementInvariant::Admission {
            entity: entity_id,
            source,
        };
        if ability.energy_cost < SimScalar::ZERO {
            return Err(admission(crate::entity::EntityAdmissionError::NegativeCost));
        }
        if ability.cast_range < SimScalar::ZERO {
            return Err(MovementInvariant::Arithmetic {
                entity: entity_id,
                operation: "ability range",
                source: MovementMathError::InvalidDistance,
            });
        }
        if entity.energy < ability.energy_cost {
            return Ok(false);
        }

        if ability.cast_range > SimScalar::ZERO {
            if let Some(pos) = target_pos {
                if !crate::movement::math::within_distance(
                    entity.position,
                    pos,
                    i128::from(ability.cast_range.raw()),
                ) {
                    return Ok(false);
                }
            }
        }

        let prepared_energy = entity
            .energy
            .checked_sub(ability.energy_cost)
            .ok_or_else(|| admission(crate::entity::EntityAdmissionError::PaymentOverflow))?;
        let mut prepared_overclock = None;
        // Validate every fallible payment/rate/placement before any commit.
        match &ability.effect {
            AbilityEffectSpec::Overclock(spec) => {
                let Some(prepared) = entity.prepare_overclock(*spec).map_err(admission)? else {
                    return Ok(false);
                };
                self.effective_max_speed(arch, entity.owner)
                    .and_then(|base| {
                        base.checked_mul(prepared.multiplier)
                            .ok_or(MovementMathError::Unrepresentable)
                    })
                    .map_err(|source| MovementInvariant::Arithmetic {
                        entity: entity_id,
                        operation: "Overclock effective speed",
                        source,
                    })?;
                prepared_overclock = Some(prepared);
            }
            AbilityEffectSpec::SiegeModeToggle { .. }
            | AbilityEffectSpec::CloakToggle { .. }
            | AbilityEffectSpec::SwitchMode { .. } => {}
            AbilityEffectSpec::BlinkTeleport { max_range, .. } => {
                let Some(dest) = target_pos else {
                    return Ok(false);
                };
                if *max_range < SimScalar::ZERO {
                    return Err(MovementInvariant::Arithmetic {
                        entity: entity_id,
                        operation: "Blink range",
                        source: MovementMathError::InvalidDistance,
                    });
                }
                if !crate::movement::math::within_distance(
                    entity.position,
                    dest,
                    i128::from(max_range.raw()),
                ) {
                    return Ok(false);
                }
                if let Err(error) =
                    self.validate_displacement_destination_prevalidated(entity_id, dest)
                {
                    if let DisplacementAdmissionError::Invariant(source) = error {
                        return Err(source);
                    }
                    return Ok(false);
                }
            }
            AbilityEffectSpec::PsionicStormAoE { .. } => {
                if target_pos.is_none() {
                    return Ok(false);
                }
            }
        }

        // All placement/ability preparation precedes even cooldown bookkeeping.
        let Some(entity) = self.entities.get_mut(&entity_id) else {
            return Ok(false);
        };
        if let Some(prepared) = prepared_overclock {
            entity.commit_overclock(prepared);
        }
        entity.energy = prepared_energy;
        if (ability_index as usize) < entity.ability_cooldowns.len() {
            entity.ability_cooldowns[ability_index as usize] = ability.cooldown_ticks;
        }

        let effect = ability.effect.clone();
        let entity_owner = entity.owner;
        match effect {
            AbilityEffectSpec::Overclock(_) => {
                // The prepared buff committed atomically with resources above.
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

        Ok(true)
    }

    // =========================================================================
    // 10-Phase Deterministic Pipeline Implementation
    // =========================================================================

    /// Phase 1: Process scheduled player input commands.
    pub fn process_commands(&mut self, input: &TickBundle) {
        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            return;
        }
        self.process_commands_prevalidated(input);
    }

    // Normal step or public dispatcher validated once. No command writes carrier links.
    fn process_commands_prevalidated(&mut self, input: &TickBundle) {
        for (command_index, cmd) in input.commands.iter().enumerate() {
            if matches!(
                cmd.order,
                UnitOrder::Move { .. } | UnitOrder::AttackMove { .. }
            ) {
                let result = self.issue_movement_prevalidated(
                    cmd.player_id,
                    &cmd.entities,
                    &cmd.order,
                    cmd.queue,
                );
                self.movement.outcomes.push(MovementOutcome {
                    tick: Tick(self.current_tick),
                    command_index,
                    player: cmd.player_id,
                    result,
                });
                continue;
            }
            if let UnitOrder::Produce { unit_type } = cmd.order {
                let result =
                    self.queue_production_prevalidated(cmd.player_id, &cmd.entities, unit_type);
                self.production
                    .outcomes
                    .push(crate::production::ProductionOutcome {
                        tick: Tick(self.current_tick),
                        command_index,
                        player: cmd.player_id,
                        result,
                    });
                continue;
            }
            let player_idx = cmd.player_id.0 as usize;
            if player_idx >= self.player_resources.len() {
                continue;
            }

            match &cmd.order {
                UnitOrder::Build {
                    structure_type,
                    cell_x,
                    cell_y,
                } => {
                    let _ = self.spawn_structure(*structure_type, cmd.player_id, *cell_x, *cell_y);
                }
                UnitOrder::UseStim => {
                    for &ent_id in &cmd.entities {
                        let Some(actor) = self
                            .entities
                            .get(&ent_id)
                            .filter(|e| e.owner == cmd.player_id)
                        else {
                            continue;
                        };
                        let spec = if let Ok(arch) = self.catalog.get(actor.unit_type_id) {
                            arch.overclock
                        } else {
                            self.report_movement_invariant(MovementInvariant::MissingArchetype(
                                ent_id,
                            ));
                            continue;
                        };
                        if let Some(spec) = spec {
                            if let Err(source) = self.apply_world_overclock(ent_id, spec) {
                                self.report_movement_invariant(source);
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
                                    if let Err(source) = ent.issue_order(cmd.order.clone(), true) {
                                        self.tick_diagnostics.push(
                                            SimulationDiagnostic::MovementInvariant {
                                                tick: Tick(self.current_tick),
                                                source: MovementInvariant::Admission {
                                                    entity: ent_id,
                                                    source,
                                                },
                                            },
                                        );
                                    }
                                } else {
                                    self.execute_ability_prevalidated(
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
                                if let Err(source) = ent.issue_order(cmd.order.clone(), cmd.queue) {
                                    self.tick_diagnostics.push(
                                        SimulationDiagnostic::MovementInvariant {
                                            tick: Tick(self.current_tick),
                                            source: MovementInvariant::Admission {
                                                entity: ent_id,
                                                source,
                                            },
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Phase 2: Update production timers, buffs, passive regeneration, and shield delays.
    ///
    /// One front production job advances per entity, including jobs enqueued in
    /// phase 1 of this tick. A zero-duration job completes on its first update;
    /// the next queued job starts advancing on the following update.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn update_production_and_buffs(&mut self) {
        self.advance_production();
        let catalog = &self.catalog;

        for entity in self.entities.values_mut() {
            // Dead entities retain their ledger until reap; passive regeneration
            // must never resurrect them or resume their production.
            if !entity.is_alive() {
                continue;
            }
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

            // Buff timers
            for buff in &mut entity.active_buffs {
                buff.remaining_ticks = buff.remaining_ticks.saturating_sub(1);
            }
            entity.active_buffs.retain(|b| b.remaining_ticks > 0);

            // Passive Vitals & Shield delay
            if let Ok(arch) = catalog.get(entity.unit_type_id) {
                entity.hp = (entity.hp + arch.hp_regen_per_tick).min(entity.max_hp);
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
        let mut hazard_flee_candidates = Vec::new();
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

                for (&entity_id, entity) in &mut self.entities {
                    if !entity.is_alive() || entity.is_loaded_in_transport() {
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

                        // Idle mobile units evacuate the persistent hazard zone
                        if matches!(entity.order_queue.front(), None | Some(UnitOrder::Idle))
                            && !entity.is_holding_position
                            && !entity.is_immobile
                            && !entity.is_loaded_in_transport()
                        {
                            hazard_flee_candidates.push((
                                entity_id,
                                entity.position,
                                hazard.center,
                                hazard.radius,
                            ));
                        }
                    }
                }
            }
        }
        for h_idx in expired_hazards.into_iter().rev() {
            self.active_hazards.remove(h_idx);
        }

        // Direct hazard evacuation moves outside the affected radius
        for (id, _pos, center, radius) in hazard_flee_candidates {
            let flee_dist = radius + SimScalar::from_num(2);
            let flee = self.compute_entity_flee_destination_prevalidated(id, center, flee_dist);
            if let Err(source) = flee {
                self.report_movement_invariant(source);
                continue;
            }
            if let Ok(Some(flee_target)) = flee {
                if let Some(unit) = self.entities.get_mut(&id) {
                    unit.order_queue.clear();
                    unit.order_queue.push_back(UnitOrder::Move {
                        target: flee_target,
                    });
                    unit.current_funnel_waypoints.clear();
                    unit.current_target = None;
                }
            }
        }

        // Hazards may have killed a ready producer; completion rechecks its life.
        self.complete_production();
    }

    /// Phase 3: Update per-player Fog of War visibility maps and remembered obstacle perception.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn update_vision(&mut self) {
        for p in 0..8 {
            self.vision_maps[p].begin_tick_update();
        }

        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() {
                continue;
            }
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

    /// Compatibility adapter for typed loading. Ordinary rejection returns false;
    /// corruption emits diagnostic evidence. Prefer `try_load_unit_into_transport`.
    pub fn load_unit_into_transport(&mut self, unit_id: EntityId, transport_id: EntityId) -> bool {
        self.try_load_unit_into_transport(unit_id, transport_id)
            .is_ok()
    }

    /// Unloads a unit from a transport craft at the specified landing position.
    pub fn unload_unit_from_transport(&mut self, unit_id: EntityId, unload_pos: SimVec2) -> bool {
        self.try_unload_unit_from_transport(unit_id, unload_pos)
            .is_ok()
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

        if !target.is_alive() || target.is_loaded_in_transport() || target.is_invulnerable {
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

    /// Finds a legacy ground/radius-one autonomous flee goal in world coordinates.
    ///
    /// # Errors
    /// Returns invalid distance/domain or unrepresentable arithmetic without mutation.
    pub fn compute_flee_destination(
        &self,
        victim_pos: SimVec2,
        threat_pos: SimVec2,
        desired_dist: SimScalar,
    ) -> Result<Option<SimVec2>, MovementMathError> {
        Self::flee_destination(
            &self.map,
            MotionPolicy {
                layer: MovementLayer::Ground,
                radius: SimScalar::ONE,
            },
            victim_pos,
            threat_pos,
            desired_dist,
        )
    }

    /// Finds an autonomous flee goal using the actor's current authored policy.
    ///
    /// # Errors
    /// Rejects malformed actor/speed/foundation state or unrepresentable geometry.
    pub fn compute_entity_flee_destination(
        &self,
        id: EntityId,
        threat: SimVec2,
        distance: SimScalar,
    ) -> Result<Option<SimVec2>, MovementInvariant> {
        self.validate_transport_links()
            .map_err(MovementInvariant::Transport)?;
        self.compute_entity_flee_destination_prevalidated(id, threat, distance)
    }

    // Caller validated graph. Loaded actors have no world-space flee goal.
    fn compute_entity_flee_destination_prevalidated(
        &self,
        id: EntityId,
        threat: SimVec2,
        distance: SimScalar,
    ) -> Result<Option<SimVec2>, MovementInvariant> {
        if self
            .entities
            .get(&id)
            .is_some_and(Entity::is_loaded_in_transport)
        {
            return Ok(None);
        }
        let entry = self.motion_entry(id)?;
        let actor = self
            .entities
            .get(&id)
            .ok_or(MovementInvariant::InvalidState {
                entity: id,
                reason: "flee actor disappeared",
            })?;
        Self::flee_destination(&self.map, entry.policy, actor.position, threat, distance).map_err(
            |source| MovementInvariant::Arithmetic {
                entity: id,
                operation: "flee destination",
                source,
            },
        )
    }

    fn flee_destination(
        map: &SimMap,
        policy: MotionPolicy,
        victim_pos: SimVec2,
        threat_pos: SimVec2,
        desired_dist: SimScalar,
    ) -> Result<Option<SimVec2>, MovementMathError> {
        use crate::movement::math::{
            checked_vec_add, checked_vec_scale, direction, squared_distance,
        };
        if desired_dist < SimScalar::ZERO || !policy.contains(map, victim_pos) {
            return Err(MovementMathError::InvalidDistance);
        }
        let threshold = (SimScalar::from_f32_lossy(0.0001).raw() as u128) << 32;
        let away = if squared_distance(victim_pos, threat_pos) > (0, threshold) {
            direction(victim_pos, threat_pos)?
        } else {
            -SimVec2::X_AXIS
        };
        let min = i128::from(policy.radius.raw());
        let max_x = (i128::from(map.width()) << 32) - min;
        let max_y = (i128::from(map.height()) << 32) - min;
        if min > max_x || min > max_y {
            return Err(MovementMathError::InvalidDistance);
        }
        let angles = [0, 45, -45, 90, -90, 135, -135];
        for dist in [
            desired_dist,
            SimScalar::from_num(4),
            SimScalar::from_num(3),
            SimScalar::from_num(2),
        ] {
            if dist > desired_dist {
                continue;
            }
            for degrees in angles {
                let dir = if degrees == 0 {
                    away
                } else {
                    let angle = SimAngle::from_degrees(degrees);
                    checked_vec_add(
                        checked_vec_scale(away, angle.cos())?,
                        checked_vec_scale(SimVec2::new(-away.y, away.x), angle.sin())?,
                    )?
                };
                let delta = checked_vec_scale(dir, dist)?;
                // Existing autonomous boundary adjustment is evaluated wide before
                // narrowing, so a legal clamped goal does not overflow on overshoot.
                let x =
                    (i128::from(victim_pos.x.raw()) + i128::from(delta.x.raw())).clamp(min, max_x);
                let y =
                    (i128::from(victim_pos.y.raw()) + i128::from(delta.y.raw())).clamp(min, max_y);
                let narrow = |raw| {
                    i64::try_from(raw)
                        .map(SimScalar::from_raw)
                        .map_err(|_| MovementMathError::Unrepresentable)
                };
                let candidate = SimVec2::new(narrow(x)?, narrow(y)?);
                if policy.permits_segment(map, victim_pos, candidate) {
                    return Ok(Some(candidate));
                }
            }
        }
        Ok(None)
    }

    /// Triggers combat retaliation aggro and allied call-for-help when a unit sustains damage.
    ///
    /// If the victim is idle, mobile, and cannot retaliate against the attacker (e.g. ground vs air,
    /// unarmed, or attacker is untargetable/undetected), the victim automatically flees directly
    /// away from the threat origin.
    pub fn apply_combat_retaliation(&mut self, victim_id: EntityId, attacker_id: EntityId) {
        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            return;
        }
        self.apply_combat_retaliation_prevalidated(victim_id, attacker_id);
    }

    // Called from valid combat/hazard phases; no relation fields are mutated.
    fn apply_combat_retaliation_prevalidated(
        &mut self,
        victim_id: EntityId,
        attacker_id: EntityId,
    ) {
        let victim_data = self
            .entities
            .get(&victim_id)
            .filter(|v| !v.is_loaded_in_transport())
            .map(|v| {
                (
                    v.position,
                    v.owner,
                    v.is_holding_position,
                    v.is_immobile,
                    v.unit_type_id,
                    matches!(v.order_queue.front(), None | Some(UnitOrder::Idle)),
                )
            });
        let Some((
            victim_pos,
            victim_owner,
            victim_holding,
            victim_immobile,
            victim_type,
            victim_is_idle,
        )) = victim_data
        else {
            return;
        };

        let attacker_data = self.entities.get(&attacker_id).map(|a| {
            (
                a.is_alive(),
                a.unit_type_id,
                a.is_cloaked,
                a.position,
                a.is_loaded_in_transport(),
            )
        });
        let Some((att_alive, att_type_id, att_cloaked, att_pos, att_loaded)) = attacker_data else {
            return;
        };
        if !att_alive {
            return;
        }

        let Ok(att_arch) = self.catalog.get(att_type_id) else {
            return;
        };

        let p_idx = victim_owner.0 as usize;
        let mut attacker_targetable = !att_loaded;

        // Check visibility of attacker to victim's owner
        if p_idx < 8 {
            if let Ok((cx, cy)) = self.map.world_to_cell(att_pos) {
                let is_vis =
                    self.vision_maps[p_idx].visibility_at(cx, cy) == VisibilityState::Visible;
                if !is_vis || (att_cloaked && !self.vision_maps[p_idx].is_detected(cx, cy)) {
                    attacker_targetable = false;
                }
            } else {
                attacker_targetable = false;
            }
        }

        let Ok(victim_arch) = self.catalog.get(victim_type) else {
            return;
        };

        let victim_can_hit = if attacker_targetable {
            let active_w = victim_arch.weapon_against(att_arch.mover_type, None);
            active_w.is_some_and(|w| w.target_flags.intersects(att_arch.attributes))
        } else {
            false
        };

        // Case B: Helpless victim auto-flee response
        if !victim_can_hit
            && victim_is_idle
            && !victim_holding
            && !victim_immobile
            && victim_arch.max_speed > SimScalar::ZERO
        {
            let flee = self.compute_entity_flee_destination_prevalidated(
                victim_id,
                att_pos,
                SimScalar::from_num(5),
            );
            if let Err(source) = &flee {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::MovementInvariant {
                        tick: Tick(self.current_tick),
                        source: source.clone(),
                    });
            }
            if let Ok(Some(flee_target)) = flee {
                if let Some(victim) = self.entities.get_mut(&victim_id) {
                    victim.order_queue.clear();
                    victim.order_queue.push_back(UnitOrder::Move {
                        target: flee_target,
                    });
                    victim.current_funnel_waypoints.clear();
                    victim.current_target = None;
                }
            }
        }

        // If attacker is not targetable, nearby allies cannot target it either
        if !attacker_targetable {
            return;
        }

        // Case A: Retaliation & Call-for-Help for units capable of fighting back
        let mut alerted_units = Vec::new();
        let call_radius_sq = SimScalar::from_num(25); // 5.0 world units squared

        for (&id, entity) in &self.entities {
            if entity.owner != victim_owner
                || !entity.is_alive()
                || entity.is_holding_position
                || entity.is_immobile
                || entity.is_loaded_in_transport()
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
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn acquire_targets(&mut self) {
        // Populate spatial hash grid with current entity positions
        self.spatial_hash.clear();
        for (id, entity) in &self.entities {
            if !entity.is_loaded_in_transport() {
                self.spatial_hash.insert(*id, entity.position);
            }
        }

        let mut auto_orders = Vec::new();

        for (id, entity) in &self.entities {
            if entity.channeling_remaining_ticks > 0 || entity.is_loaded_in_transport() {
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
                        if !current_t.is_alive() || current_t.is_loaded_in_transport() {
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
                    || neighbor.is_loaded_in_transport()
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

    /// Phase5: Prepare and atomically commit each actor's checked movement operation.
    /// Malformed state emits diagnostics without partial pose, route or order edits.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn step_movement_and_steering(&mut self) {
        self.advance_checked_movement();
    }

    /// Checks whether `target_pos` is valid and passable for a ground unit currently at `current_pos`.
    #[must_use]
    #[inline]
    pub fn is_position_pathable_from(&self, current_pos: SimVec2, target_pos: SimVec2) -> bool {
        Self::is_position_pathable(&self.map, current_pos, target_pos)
    }

    /// Checks the complete ground-motion segment in fixed-point world units.
    ///
    /// Every crossed terrain edge must be connected and every entered cell must be
    /// free of structures. This also applies to long flee candidates and collision pushes.
    #[must_use]
    pub fn is_position_pathable(map: &SimMap, current_pos: SimVec2, target_pos: SimVec2) -> bool {
        map.ground_segment_is_connected(current_pos, target_pos, |_, next| {
            map.cell(next.0, next.1).is_ok_and(|cell| {
                cell.flags.contains(CellFlags::GROUND_PATHABLE)
                    && !cell.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE)
            })
        })
    }

    /// Phase 6: Resolve circle-circle collisions, engagement anchoring, and push priority.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn resolve_collisions_and_push(&mut self) {
        let mut entries = std::mem::take(&mut self.movement.collision_entries);
        entries.clear();
        self.spatial_hash.clear();
        let mut max_radii = [SimScalar::ZERO; 2];
        for (id, entity) in &self.entities {
            if entity.is_alive() && !entity.is_loaded_in_transport() {
                match self.motion_entry(*id) {
                    Ok(entry) => {
                        let policy = entry.policy;
                        entries.push(entry);
                        max_radii[policy.layer.index()] =
                            max_radii[policy.layer.index()].max(policy.radius);
                        self.spatial_hash.insert(*id, entity.position);
                    }
                    Err(source) => {
                        self.tick_diagnostics
                            .push(SimulationDiagnostic::MovementInvariant {
                                tick: Tick(self.current_tick),
                                source,
                            });
                    }
                }
            }
        }

        let mut friendly_pushes: BTreeMap<EntityId, SimVec2> = BTreeMap::new();
        let mut hard_pushes: BTreeMap<EntityId, SimVec2> = BTreeMap::new();

        for entry_a in &entries {
            let id_a = entry_a.id;
            let Some(entity_a) = self.entities.get(&id_a) else {
                continue;
            };
            if !entity_a.is_alive()
                || entity_a.is_mineral_walking
                || entity_a.is_loaded_in_transport()
            {
                continue;
            }

            let policy_a = entry_a.policy;
            let radius_a = policy_a.radius;

            let pos_a = entity_a.position;
            let vel_a = entity_a.velocity;
            let owner_a = entity_a.owner;
            let is_holding_a = entity_a.is_holding_position;
            let priority_a = self
                .catalog
                .get(entity_a.unit_type_id)
                .map_or(0, |a| a.push_priority);

            let has_attack_target_a = entity_a
                .current_target
                .or_else(|| {
                    if let Some(UnitOrder::AttackTarget { target }) = entity_a.order_queue.front() {
                        Some(*target)
                    } else {
                        None
                    }
                })
                .is_some_and(|tid| self.entities.get(&tid).is_some_and(Entity::is_alive));
            let is_engaged_a = matches!(
                entity_a.weapon_phase,
                WeaponPhase::RotatingToTarget { .. }
                    | WeaponPhase::Windup { .. }
                    | WeaponPhase::Backswing { .. }
            ) || (has_attack_target_a
                && entity_a.order_queue.front().is_some_and(|o| {
                    matches!(
                        o,
                        UnitOrder::AttackTarget { .. } | UnitOrder::AttackMove { .. }
                    )
                })
                && entity_a.velocity == SimVec2::ZERO);

            self.spatial_hash.query_radius(
                pos_a,
                crate::movement::conservative_query_radius(
                    &self.map,
                    radius_a,
                    max_radii[policy_a.layer.index()],
                ),
                &mut self.scratch_neighbors,
            );

            for &id_b in &self.scratch_neighbors {
                if id_b.0 <= id_a.0 {
                    continue;
                }

                let Some(entity_b) = self.entities.get(&id_b) else {
                    continue;
                };
                if !entity_b.is_alive()
                    || entity_b.is_mineral_walking
                    || entity_b.is_loaded_in_transport()
                {
                    continue;
                }

                let Ok(index_b) = entries.binary_search_by_key(&id_b, |entry| entry.id) else {
                    continue;
                };
                let policy_b = entries[index_b].policy;
                if policy_a.layer != policy_b.layer {
                    continue;
                }
                let radius_b = policy_b.radius;
                if crate::movement::circles_overlap(pos_a, radius_a, entity_b.position, radius_b) {
                    let arithmetic = radius_a.checked_add(radius_b).and_then(|combined| {
                        crate::movement::checked_distance(pos_a, entity_b.position)
                            .map(|dist| (combined, dist))
                    });
                    let Some((combined, dist)) = arithmetic else {
                        self.tick_diagnostics
                            .push(SimulationDiagnostic::MovementInvariant {
                            tick: Tick(self.current_tick),
                            source: MovementInvariant::InvalidState {
                                entity: id_a,
                                reason:
                                    "collision geometry exceeds fixed-point local push arithmetic",
                            },
                        });
                        continue;
                    };

                    let priority_b = self
                        .catalog
                        .get(entity_b.unit_type_id)
                        .map_or(0, |a| a.push_priority);

                    let has_attack_target_b = entity_b
                        .current_target
                        .or_else(|| {
                            if let Some(UnitOrder::AttackTarget { target }) =
                                entity_b.order_queue.front()
                            {
                                Some(*target)
                            } else {
                                None
                            }
                        })
                        .is_some_and(|tid| self.entities.get(&tid).is_some_and(Entity::is_alive));
                    let is_engaged_b = matches!(
                        entity_b.weapon_phase,
                        WeaponPhase::RotatingToTarget { .. }
                            | WeaponPhase::Windup { .. }
                            | WeaponPhase::Backswing { .. }
                    ) || (has_attack_target_b
                        && entity_b.order_queue.front().is_some_and(|o| {
                            matches!(
                                o,
                                UnitOrder::AttackTarget { .. } | UnitOrder::AttackMove { .. }
                            )
                        })
                        && entity_b.velocity == SimVec2::ZERO);

                    let eff_cannot_be_pushed_a = entity_a.cannot_be_pushed()
                        || (owner_a == entity_b.owner
                            && is_engaged_a
                            && (priority_a >= priority_b
                                || (entity_a.velocity == SimVec2::ZERO
                                    && entity_b.velocity != SimVec2::ZERO)));
                    let eff_cannot_be_pushed_b = entity_b.cannot_be_pushed()
                        || (owner_a == entity_b.owner
                            && is_engaged_b
                            && (priority_b >= priority_a
                                || (entity_b.velocity == SimVec2::ZERO && vel_a != SimVec2::ZERO)));

                    // SC2 Push Rule: PushPriority lateral yielding applies only when a moving
                    // unit contacts a stationary (Idle) unit. When two friendly dynamic units
                    // are both moving, contact resolution is purely radial along the line of centers.
                    let both_moving_dynamic = owner_a == entity_b.owner
                        && vel_a != SimVec2::ZERO
                        && entity_b.velocity != SimVec2::ZERO
                        && !eff_cannot_be_pushed_a
                        && !eff_cannot_be_pushed_b;

                    let a_params = PushEntityParams {
                        id: id_a,
                        pos: pos_a,
                        vel: if both_moving_dynamic {
                            SimVec2::ZERO
                        } else {
                            vel_a
                        },
                        owner: owner_a,
                        is_holding: is_holding_a,
                        push_priority: priority_a,
                        cannot_be_pushed: eff_cannot_be_pushed_a,
                    };
                    let b_params = PushEntityParams {
                        id: id_b,
                        pos: entity_b.position,
                        vel: if both_moving_dynamic {
                            SimVec2::ZERO
                        } else {
                            entity_b.velocity
                        },
                        owner: entity_b.owner,
                        is_holding: entity_b.is_holding_position,
                        push_priority: priority_b,
                        cannot_be_pushed: eff_cannot_be_pushed_b,
                    };
                    let mobility = |actor: &Entity| {
                        if actor.is_immobile
                            || actor.unpushable
                            || self.map.structures().contains_key(&actor.id)
                        {
                            CollisionMobility::Fixed
                        } else {
                            CollisionMobility::Dynamic
                        }
                    };
                    let (delta_a, delta_b) = match compute_push_displacement_with_mobility(
                        a_params,
                        b_params,
                        dist,
                        combined,
                        mobility(entity_a),
                        mobility(entity_b),
                    ) {
                        Ok(displacement) => displacement,
                        Err(source) => {
                            self.tick_diagnostics
                                .push(SimulationDiagnostic::MovementInvariant {
                                    tick: Tick(self.current_tick),
                                    source: MovementInvariant::PushGeometry {
                                        entity: id_a,
                                        source,
                                    },
                                });
                            continue;
                        }
                    };

                    let pushes = if owner_a == entity_b.owner {
                        &mut friendly_pushes
                    } else {
                        &mut hard_pushes
                    };
                    if let Err(source) = crate::movement::collision::accumulate_pair(
                        pushes, id_a, id_b, delta_a, delta_b,
                    ) {
                        self.tick_diagnostics
                            .push(SimulationDiagnostic::MovementInvariant {
                                tick: Tick(self.current_tick),
                                source: MovementInvariant::Arithmetic {
                                    entity: id_a,
                                    operation: "collision pair accumulation",
                                    source,
                                },
                            });
                    }
                }
            }
        }

        // Preserve the established friendly base-speed cap; upgrades/buffs were
        // validated at entry but this round does not rebalance crowd correction.
        for (hard, pushes) in [(false, friendly_pushes), (true, hard_pushes)] {
            for (id, push) in pushes {
                let Ok(index) = entries.binary_search_by_key(&id, |entry| entry.id) else {
                    continue;
                };
                let policy = entries[index].policy;
                let Some(actor) = self.entities.get_mut(&id) else {
                    continue;
                };
                if actor.is_immobile || actor.unpushable || self.map.structures().contains_key(&id)
                {
                    continue;
                }
                let proposed = if hard {
                    crate::movement::math::checked_vec_add(actor.position, push)
                } else {
                    let speed = match self.catalog.get(actor.unit_type_id) {
                        Ok(arch) => arch.max_speed,
                        Err(_) => continue,
                    };
                    crate::movement::collision::soft_position(
                        actor.position,
                        actor.pos_at_tick_start,
                        push,
                        speed,
                    )
                };
                let proposed = match proposed {
                    Ok(position) => position,
                    Err(source) => {
                        self.tick_diagnostics
                            .push(SimulationDiagnostic::MovementInvariant {
                                tick: Tick(self.current_tick),
                                source: MovementInvariant::Arithmetic {
                                    entity: id,
                                    operation: "collision application",
                                    source,
                                },
                            });
                        continue;
                    }
                };
                let accepted = if policy.permits_segment(&self.map, actor.position, proposed) {
                    Some(proposed)
                } else {
                    let x = SimVec2::new(proposed.x, actor.position.y);
                    let y = SimVec2::new(actor.position.x, proposed.y);
                    if policy.permits_segment(&self.map, actor.position, x) {
                        Some(x)
                    } else if policy.permits_segment(&self.map, actor.position, y) {
                        Some(y)
                    } else {
                        None
                    }
                };
                if let Some(position) = accepted {
                    actor.position = position;
                }
            }
        }
        self.movement.collision_entries = entries;
    }

    /// Phase 7: Advance weapon state machines, fire attacks, and step projectiles.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
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
            // Retain authored weapon-selection metadata, but never expose cargo
            // coordinates to aim/damage. Air-only windup timing must not silently
            // switch to the missing-target Ground fallback when a target boards.
            target_snapshots.insert(
                id,
                (
                    entity.position,
                    radius,
                    mover_type,
                    entity.is_alive() && !entity.is_loaded_in_transport(),
                ),
            );
        }

        let mut attacks_to_apply = Vec::new();

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
            let player_idx = (entity.owner.0 as usize).min(7);
            let delta = self.player_upgrades[player_idx]
                .unit_stat_modifiers
                .get(&arch.slug)
                .map_or(0, |modifiers| modifiers.attack_cooldown_delta);
            weapon.period_ticks = resolve_attack_period(weapon.period_ticks, delta, attack_mult);

            let fired = step_entity_weapon(entity, &weapon, target_info);
            if fired {
                if let Some(target) = target_opt_id
                    .filter(|id| target_snapshots.get(id).is_some_and(|snapshot| snapshot.3))
                {
                    attacks_to_apply.push((
                        *id,
                        target,
                        weapon,
                        arch.slug.clone(),
                        entity.position,
                        entity.owner,
                    ));
                }
            }
        }

        // Apply attacks
        for (attacker_id, target_id, weapon, attacker_slug, attacker_pos, attacker_owner) in
            attacks_to_apply
        {
            // The prepared shot retains actual source provenance. Earlier attacks may
            // damage this shooter or change its orders, but cannot move/change owner
            // or reap it before this dispatch phase finishes.
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
                        owner: attacker_owner,
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
                        self.apply_combat_retaliation_prevalidated(target_id, attacker_id);

                        // Apply circular splash if defined for hitscan/instant attack
                        if let Some(splash) = &weapon.splash {
                            let mut splash_hits = Vec::new();
                            for (&other_id, other) in &self.entities {
                                if other_id == target_id
                                    || other_id == attacker_id
                                    || !other.is_alive()
                                    || other.is_loaded_in_transport()
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
                                self.apply_combat_retaliation_prevalidated(spl_id, attacker_id);
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
                                    owner: attacker_owner,
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
                                    owner: attacker_owner,
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
                                    owner: attacker_owner,
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
                                        owner: attacker_owner,
                                    });
                            }
                            _ => {}
                        }

                        let _ = target_owner;
                    }
                }
            }

            // Self-destruct if weapon is suicidal
            if weapon.suicide_on_attack {
                if let Some(attacker) = self.entities.get_mut(&attacker_id) {
                    attacker.hp = SimScalar::ZERO;
                    attacker.shields = SimScalar::ZERO;
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
                                                best_cand.as_ref().is_none_or(|b| d2 < b.3);
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
                            self.apply_combat_retaliation_prevalidated(target_id, attacker_id);
                        }

                        // Apply circular splash if defined
                        if let Some(splash) = &proj.weapon_spec.splash {
                            let mut splash_hits = Vec::new();
                            for (&other_id, other) in &self.entities {
                                if other_id == target_id
                                    || !other.is_alive()
                                    || other.is_loaded_in_transport()
                                {
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
                                if let Some(attacker_id) = proj.attacker_id {
                                    self.apply_combat_retaliation_prevalidated(spl_id, attacker_id);
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
                    if !other.is_alive() || other.is_loaded_in_transport() {
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
                    if let Some(attacker_id) = proj.attacker_id {
                        self.apply_combat_retaliation_prevalidated(spl_id, attacker_id);
                    }
                }
            }
        }
    }

    /// Phase 8: Reap destroyed entities and emit death shatter visuals.
    pub fn reap_dead_and_emit_shatter(&mut self) {
        self.reap_transport_groups();
    }

    /// Phase 9: Compute the versioned 64-bit dynamic-state checksum.
    ///
    /// Includes persistent gameplay state and live RNG position. Catalog equality
    /// must be established separately; see [`compute_simulation_checksum`].
    #[must_use]
    pub fn compute_checksum(&self) -> u64 {
        compute_simulation_checksum(self)
    }

    /// Phase 10: Build immutable render snapshot for client visualization.
    pub fn build_render_snapshot(&mut self) -> RenderSnapshot {
        let mut output = RenderSnapshot::default();
        self.write_render_snapshot(&mut output);
        output
    }

    /// Writes the current immutable visual state into caller-owned reusable buffers.
    ///
    /// Entity records are ordered by entity ID. This consumes pending visual events
    /// exactly once, preserving both buffers' capacity; it is not a per-frame getter.
    /// No simulation phase executes and authoritative state/checksum are unchanged.
    pub fn write_render_snapshot(&mut self, output: &mut RenderSnapshot) {
        // PERFORMANCE RATIONALE:
        // Why: Paired client snapshots should reuse storage across ordinary ticks.
        // What: Clear and refill entity records, then move event elements without
        // replacing either Vec allocation. Reserve only for actual entity growth.
        // Trade-off: Callers must consume output events before reusing the buffer.
        output.bubbles.clear();
        output.bubbles.reserve(self.entities.len());
        output.combat_events.clear();
        output.tick = Tick(self.current_tick);
        output.checksum = self.last_checksum;

        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() {
                continue;
            }
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

            let hp_ratio = if entity.max_hp > SimScalar::ZERO {
                (entity.hp / entity.max_hp).to_f32().clamp(0.0, 1.0)
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

            output.bubbles.push(BubbleRenderInstance {
                entity_id: entity.id,
                shape: arch.shape,
                silhouette: arch.silhouette,
                unit_type: entity.unit_type_id,
                owner: entity.owner,
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

        output.combat_events.append(&mut self.pending_events);
    }
}
