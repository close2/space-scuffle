//! # Authoritative Simulation World & 10-Phase Tick Pipeline
//!
//! Encapsulates all authoritative simulation components in [`SimWorld`], driving
//! the deterministic lockstep loop through an invariant 10-phase execution pipeline at 25 Hz.
//! Phase 2 includes unit tasks ([`crate::economy`]); phase 8 settles the match
//! ([`crate::match_state`]).
//!
//! ## Invariants Maintained
//! 1. All calculations use deterministic fixed-point math (`SimScalar`, `SimVec2`, `SimAngle`).
//! 2. Entity iteration order is strictly sorted by `EntityId` to guarantee identical checksums.
//! 3. Simulation ticks run at an invariant rate of 25 Hz (40 ms per tick).
//! 4. Zero engine / GPU dependencies; compiles cleanly in headless CI.

use crate::catalog::{UnitArchetype, UnitCatalog, WeaponSpec};
use crate::checksum::compute_simulation_checksum;
use crate::combat::TargetPriorityKey;
use crate::economy::{GameCommandError, GameCommandOutcome};
use crate::entity::{ActiveProjectile, Entity, WeaponPhase};
use crate::error::{SimCoreError, SimResult};
use crate::movement::{
    DisplacementAdmissionError, MotionPolicy, MovementInvariant, MovementLayer, MovementMathError,
    MovementOutcome,
};
use crate::steering::{SpatialHashGrid, SPATIAL_CELL_SIZE};
use crate::upgrades::{UpgradeCatalog, OVERCLOCK_ABILITY_SLUG};
use crate::weapon_timing::{resolve_attack_period, AttackRate, SimulationDiagnostic};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{
    watchtower_source, CellFlags, CliffLevel, GameMap, MapCell, Pathfinder, PlayerVisionMap,
    SimMap, VisionSource,
};
use sim_types::{
    AbilityEffectSpec, AttributeFlags, BubbleRenderInstance, CombatEventVisual, EntityId, PlayerId,
    PlayerStatusView, PlayerUpgradeState, RenderSnapshot, ResourceNodeId, ResourceNodeView,
    ShapeKind, SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
    VisibilityState, WeaponVisualKind,
};
use std::collections::BTreeMap;

// PERFORMANCE RATIONALE:
// Why: Allocating intermediate vectors during the 10-phase tick loop causes allocator fragmentation
// and degrades execution speed from >10,000 ticks/sec down to <2,000 ticks/sec.
// What: Persistent scratch buffers (spatial query results, candidate targets, dead entity lists)
// are stored directly inside `SimWorld` and cleared each tick without heap reallocation.
// Trade-off: Slightly larger persistent struct footprint in exchange for zero per-tick heap allocations.

/// Squared distance, in raw Q64.64, within which a threat counts as standing on its victim.
/// Fixed at 0.0001 world units squared, in the same widened scale as `squared_distance`.
const FLEE_COINCIDENCE_SQUARED: u128 = 429_497 << 32;

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

/// One sight source already counted into a player's vision map.
#[derive(Clone, Copy)]
struct AppliedSight {
    id: EntityId,
    owner: usize,
    source: VisionSource,
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
    pub(crate) economy: crate::economy::EconomyWorkspace,
    /// Authoritative standing of the match: participants, defeats and result.
    pub match_state: crate::match_state::MatchState,
    pub(crate) combat: crate::combat_resolution::CombatWorkspace,
    /// Active continuous hazard/AoE zones (e.g. Psionic Storm) ticking in the world.
    pub active_hazards: Vec<ActiveHazardZone>,
    /// Monotonically increasing hazard ID generator.
    pub next_hazard_id: u32,
    /// Whether pathfinding respects fog-of-war vision and known obstacles.
    pub vision_aware_pathfinding: bool,
    /// Per-player known blocked cells (structures seen in visible cells and remembered through fog).
    pub known_blocked_cells: [Vec<bool>; 8],

    // Reusable per-tick scratch buffers to eliminate allocations
    pub(crate) scratch_neighbors: Vec<EntityId>,
    pub(crate) scratch_targets: Vec<(TargetPriorityKey, EntityId)>,
    pub(crate) scratch_acquisitions: Vec<(EntityId, crate::target_acquisition::Acquisition)>,
    scratch_watchtower_units: Vec<(PlayerId, SimVec2)>,
    scratch_sights: Vec<AppliedSight>,
    /// Sources currently included in `vision_maps`, sorted by ID.
    counted_sights: Vec<AppliedSight>,
    /// Map cell revision `counted_sights` was cast on; `None` until the first count.
    counted_cell_revision: Option<u64>,
    /// Each vision map's recount revision when `counted_sights` was last in step with it.
    counted_recounts: [u64; 8],
    /// Indices of the cells set in each player's `known_blocked_cells`.
    known_blocked_lists: [Vec<usize>; 8],
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
            economy: crate::economy::EconomyWorkspace::default(),
            match_state: crate::match_state::MatchState::default(),
            combat: crate::combat_resolution::CombatWorkspace::default(),
            active_hazards: Vec::new(),
            next_hazard_id: 1,
            vision_aware_pathfinding: true,
            known_blocked_cells,
            scratch_neighbors: Vec::with_capacity(32),
            scratch_targets: Vec::with_capacity(64),
            scratch_acquisitions: Vec::with_capacity(64),
            scratch_watchtower_units: Vec::new(),
            scratch_sights: Vec::new(),
            counted_sights: Vec::new(),
            counted_cell_revision: None,
            counted_recounts: [0; 8],
            known_blocked_lists: Default::default(),
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
    /// - Blue Base: `CommandDome` at (5, 5), `SupplyDome` at (12, 5), `BarracksDome` at (16, 5).
    /// - Red Base: `CommandDome` at (54, 54), `SupplyDome` at (48, 54), `BarracksDome` at (42, 54).
    ///
    /// # Panics
    /// Panics if standard starter archetypes are missing from the embedded catalog.
    #[must_use]
    #[allow(clippy::expect_used)]
    #[expect(
        clippy::too_many_lines,
        reason = "split in plan steps 3.1 and 9.1, which own this file's structure"
    )]
    pub fn new_default_battle_sandbox() -> Self {
        let default_cell = MapCell::default_ground(CliffLevel(0));
        let map = GameMap::new(64, 64, default_cell).expect("invariant: 64x64 is a valid map size");
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
    /// Returns typed errors for unknown or already completed research, or for
    /// unrepresentable cumulative/live stats.
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
        if staged_player.completed_upgrade_slugs.contains(upgrade_slug) {
            return Err(SimCoreError::UpgradeAlreadyResearched(
                upgrade_slug.to_string(),
            ));
        }
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

    pub(crate) fn effective_range_from_upgrades(
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

    /// Computes the sight radius in world units, including upgrades.
    #[must_use]
    pub fn effective_sight_radius(&self, archetype: &UnitArchetype, owner: PlayerId) -> SimScalar {
        Self::sight_radius_from_upgrades(&self.player_upgrades, archetype, owner)
    }

    fn sight_radius_from_upgrades(
        upgrades: &[PlayerUpgradeState; 8],
        archetype: &UnitArchetype,
        owner: PlayerId,
    ) -> SimScalar {
        let delta = upgrades[(owner.0 as usize).min(7)]
            .unit_stat_modifiers
            .get(&archetype.slug)
            .map_or(SimScalar::ZERO, |m| m.sight_delta);
        (archetype.sight_radius + delta).max(SimScalar::ZERO)
    }

    /// Whether `owner` may use `ability_slug` of `archetype`.
    ///
    /// Abilities no upgrade unlocks are always available. Otherwise the owner must have
    /// completed an upgrade that unlocks this one.
    #[must_use]
    pub fn is_ability_unlocked(
        &self,
        archetype: &UnitArchetype,
        owner: PlayerId,
        ability_slug: &str,
    ) -> bool {
        !self
            .upgrade_catalog
            .unlocks_ability(&archetype.slug, ability_slug)
            || self
                .player_upgrade_state(owner)
                .unlocked_abilities
                .get(&archetype.slug)
                .is_some_and(|unlocked| unlocked.contains(ability_slug))
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
        self.clear_economy_outcomes();
        self.movement.outcomes.clear();
        self.movement.route_outcomes.clear();
        self.current_tick = self.current_tick.wrapping_add(1);

        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            self.last_checksum = self.compute_checksum();
            self.write_render_snapshot(output);
            return;
        }
        self.process_commands_prevalidated(input);
        self.update_production_and_buffs();
        self.update_unit_tasks();
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
    #[expect(
        clippy::too_many_lines,
        reason = "split in plan steps 3.1 and 9.1, which own this file's structure"
    )]
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

        let Some(ability) = arch.abilities.get(ability_index as usize) else {
            return Ok(false);
        };
        if !self.is_ability_unlocked(arch, entity.owner, &ability.name) {
            return Ok(false);
        }

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
            AbilityEffectSpec::SiegeModeToggle { cast_ticks, .. } => {
                if cast_ticks > 0 {
                    entity.begin_mode_channel(cast_ticks, ability_index, target_pos, target_entity);
                } else {
                    entity.apply_mode_transition(arch, &effect);
                }
                entity.velocity = SimVec2::ZERO;
                entity.current_funnel_waypoints.clear();
            }
            AbilityEffectSpec::SwitchMode { ref mode_name } => {
                if let Some(spec) = arch.get_mode(mode_name) {
                    if spec.cast_ticks > 0 {
                        entity.begin_mode_channel(
                            spec.cast_ticks,
                            ability_index,
                            target_pos,
                            target_entity,
                        );
                    } else {
                        entity.apply_mode_transition(arch, &effect);
                    }
                    entity.velocity = SimVec2::ZERO;
                    entity.current_funnel_waypoints.clear();
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
    #[expect(
        clippy::too_many_lines,
        reason = "split in plan steps 3.1 and 9.1, which own this file's structure"
    )]
    fn process_commands_prevalidated(&mut self, input: &TickBundle) {
        for (command_index, cmd) in input.commands.iter().enumerate() {
            let closed = if self.match_state.result.is_over() {
                Some(GameCommandError::MatchOver)
            } else if self.match_state.is_defeated(cmd.player_id) {
                Some(GameCommandError::PlayerDefeated(cmd.player_id))
            } else {
                None
            };
            if let Some(error) = closed {
                self.economy.outcomes.push(GameCommandOutcome {
                    tick: Tick(self.current_tick),
                    command_index,
                    player: cmd.player_id,
                    result: Err(error),
                });
                continue;
            }
            if matches!(
                cmd.order,
                UnitOrder::Build { .. }
                    | UnitOrder::Construct { .. }
                    | UnitOrder::CancelConstruction
                    | UnitOrder::Harvest { .. }
                    | UnitOrder::ReturnCargo
                    | UnitOrder::CancelProduction { .. }
                    | UnitOrder::Load { .. }
                    | UnitOrder::UnloadAll
            ) {
                let result = self.admit_game_command(cmd);
                if let Err(GameCommandError::Invariant(source)) = &result {
                    self.report_production_failure(source.clone());
                }
                self.economy.outcomes.push(GameCommandOutcome {
                    tick: Tick(self.current_tick),
                    command_index,
                    player: cmd.player_id,
                    result,
                });
                continue;
            }
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
            if let UnitOrder::Produce { .. } | UnitOrder::Research { .. } = cmd.order {
                let result = match cmd.order {
                    UnitOrder::Research { upgrade } => {
                        self.queue_research_prevalidated(cmd.player_id, &cmd.entities, upgrade)
                    }
                    UnitOrder::Produce { unit_type } => {
                        self.queue_production_prevalidated(cmd.player_id, &cmd.entities, unit_type)
                    }
                    _ => continue,
                };
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
                            arch.overclock.filter(|_| {
                                self.is_ability_unlocked(arch, actor.owner, OVERCLOCK_ABILITY_SLUG)
                            })
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
    #[expect(
        clippy::too_many_lines,
        reason = "split in plan steps 3.1 and 9.1, which own this file's structure"
    )]
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
                    if let Some((ability_index, _, _)) = entity.pending_ability.take() {
                        if let Ok(arch) = catalog.get(entity.unit_type_id) {
                            match arch.abilities.get(usize::from(ability_index)) {
                                Some(ability) => {
                                    entity.apply_mode_transition(arch, &ability.effect);
                                }
                                None => entity.leave_mode(arch),
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
                                entity.leave_mode(arch);
                            } else {
                                entity.energy -= drain_per_tick;
                            }
                        }
                    }
                }

                entity.ticks_since_last_damage = entity.ticks_since_last_damage.saturating_add(1);
                if entity.ticks_since_last_damage >= 250 && entity.is_completed {
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
                    if !Self::is_damageable(entity) {
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

    /// Assigns each watchtower from the ground units standing on the map this tick.
    ///
    /// Air units overfly towers, and structures and transported cargo do not occupy them.
    // PERFORMANCE RATIONALE:
    // Why: This runs every tick, and most maps have few towers while battles have many units.
    // What: Skip entirely on towerless maps; otherwise reuse one scratch buffer of positions.
    // Trade-off: One retained allocation in exchange for no per-tick heap use.
    fn update_watchtower_control(&mut self) {
        if self.map.watchtowers().is_empty() {
            return;
        }
        self.scratch_watchtower_units.clear();
        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() || !entity.is_completed {
                continue;
            }
            let occupies = self.catalog.get(entity.unit_type_id).is_ok_and(|arch| {
                matches!(
                    arch.mover_type,
                    sim_types::MoverType::Ground | sim_types::MoverType::CliffJumper
                ) && !arch.is_immobile_by_default()
            });
            if occupies {
                self.scratch_watchtower_units
                    .push((entity.owner, entity.position));
            }
        }
        self.map
            .update_watchtower_capture(&self.scratch_watchtower_units);
    }

    /// Lists every sight source for this tick in ID order, watchtowers after all entities.
    fn collect_sights(&self, sights: &mut Vec<AppliedSight>) {
        sights.clear();
        for entity in self.entities.values() {
            if entity.is_loaded_in_transport() || usize::from(entity.owner.0) >= 8 {
                continue;
            }
            if let Ok(arch) = self.catalog.get(entity.unit_type_id) {
                sights.push(AppliedSight {
                    id: entity.id,
                    owner: usize::from(entity.owner.0),
                    source: VisionSource {
                        pos: entity.position,
                        mover_type: arch.mover_type,
                        sight_radius: Self::sight_radius_from_upgrades(
                            &self.player_upgrades,
                            arch,
                            entity.owner,
                        ),
                        is_detector: arch.is_detector,
                    },
                });
            }
        }
        // Towers take the top of the ID range, in ascending order to keep the list sorted.
        for (index, tower) in self.map.watchtowers().iter().enumerate().rev() {
            let Some(owner) = tower.controlling_player.map(|p| usize::from(p.0)) else {
                continue;
            };
            let Ok(offset) = u32::try_from(index) else {
                continue;
            };
            if owner < 8 {
                sights.push(AppliedSight {
                    id: EntityId(u32::MAX - offset),
                    owner,
                    source: watchtower_source(tower),
                });
            }
        }
    }

    /// Whether two sources see exactly the same cells, so nothing need be recast.
    ///
    /// Height only varies inside a ramp cell, so elsewhere the cell stands in for the position.
    fn sees_the_same(&self, a: &VisionSource, b: &VisionSource) -> bool {
        if (a.mover_type, a.sight_radius, a.is_detector)
            != (b.mover_type, b.sight_radius, b.is_detector)
        {
            return false;
        }
        if a.pos == b.pos {
            return true;
        }
        let cell = self.map.world_to_cell(a.pos);
        cell.is_ok()
            && cell == self.map.world_to_cell(b.pos)
            && cell.is_ok_and(|(x, y)| self.map.cell(x, y).is_ok_and(|c| c.ramp.is_none()))
    }

    /// Phase 3: Update per-player Fog of War visibility maps and remembered obstacle perception.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    // PERFORMANCE RATIONALE:
    // Why: Casting every unit's sight each tick was the largest cost of a 400-unit battle, and
    // settling and resetting eight maps cost O(map area) even with no units at all.
    // What: A unit's sight only changes when it enters a new cell, about one tick in ten. Keep
    // the sources already counted and recast only those that changed, as a removal plus an
    // addition, settling just the cells they touch.
    // Trade-off: One retained list of sources, and a full recount whenever terrain or a map is
    // edited outside this function. Counters and states end exactly as a full recount leaves them.
    pub fn update_vision(&mut self) {
        self.update_watchtower_control();

        let mut sights = std::mem::take(&mut self.scratch_sights);
        self.collect_sights(&mut sights);
        let mut counted = std::mem::take(&mut self.counted_sights);
        let in_step = self.counted_cell_revision == Some(self.map.cell_revision())
            && (0..8).all(|p| self.vision_maps[p].recount_revision() == self.counted_recounts[p]);
        if in_step {
            self.recast_changed_sights(&counted, &sights);
        } else {
            for vision in &mut self.vision_maps {
                vision.begin_tick_update();
            }
            for sight in &sights {
                self.vision_maps[sight.owner].apply_vision_source(&self.map, &sight.source);
            }
            for vision in &mut self.vision_maps {
                vision.end_tick_update();
            }
            self.counted_cell_revision = Some(self.map.cell_revision());
            for p in 0..8 {
                self.counted_recounts[p] = self.vision_maps[p].recount_revision();
            }
        }
        std::mem::swap(&mut counted, &mut sights);
        self.counted_sights = counted;
        self.scratch_sights = sights;

        self.update_known_structures();
    }

    /// Walks the old and new ID-sorted source lists together, recasting only the differences.
    fn recast_changed_sights(&mut self, counted: &[AppliedSight], sights: &[AppliedSight]) {
        let (mut old, mut new) = (counted.iter().peekable(), sights.iter().peekable());
        loop {
            match (old.peek().copied(), new.peek().copied()) {
                (Some(gone), next) if next.is_none_or(|n| gone.id < n.id) => {
                    self.vision_maps[gone.owner].remove_vision_source(&self.map, &gone.source);
                    old.next();
                }
                (Some(before), Some(after)) if before.id == after.id => {
                    if before.owner != after.owner
                        || !self.sees_the_same(&before.source, &after.source)
                    {
                        self.vision_maps[before.owner]
                            .remove_vision_source(&self.map, &before.source);
                        self.vision_maps[after.owner].add_vision_source(&self.map, &after.source);
                    }
                    old.next();
                    new.next();
                }
                (_, Some(added)) => {
                    self.vision_maps[added.owner].add_vision_source(&self.map, &added.source);
                    new.next();
                }
                _ => return,
            }
        }
    }

    /// Updates each player's memory of structures from the cells it sees now.
    // PERFORMANCE RATIONALE:
    // Why: Scanning every visible cell of every player is O(map area) per tick, for a record that
    // can only change at a structure's footprint or at a cell already remembered as blocked.
    // What: Visit those two sets alone, tracked per player in `known_blocked_lists`.
    // Trade-off: One retained index list per player, bounded by the structure cells on the map.
    fn update_known_structures(&mut self) {
        let width = self.map.width() as usize;
        let total_cells = width * self.map.height() as usize;
        let mut active_players = [false; 8];
        for ent in self.entities.values() {
            if let Some(active) = active_players.get_mut(usize::from(ent.owner.0)) {
                *active = true;
            }
        }
        for (p, &is_active) in active_players.iter().enumerate() {
            if !is_active {
                continue;
            }
            if self.known_blocked_cells[p].len() != total_cells {
                self.known_blocked_cells[p].resize(total_cells, false);
            }
            for structure in self.map.structures().values() {
                let (left, top) = structure.top_left;
                for y in top..top + u32::from(structure.height) {
                    for x in left..left + u32::from(structure.width) {
                        let idx = y as usize * width + x as usize;
                        if self.vision_maps[p].visibility_at(x, y) == VisibilityState::Visible
                            && !self.known_blocked_cells[p][idx]
                        {
                            self.known_blocked_cells[p][idx] = true;
                            self.known_blocked_lists[p].push(idx);
                            self.movement.recheck_routes[p] = true;
                        }
                    }
                }
            }
            let mut list = std::mem::take(&mut self.known_blocked_lists[p]);
            list.retain(|&idx| {
                let (Ok(x), Ok(y)) = (u32::try_from(idx % width), u32::try_from(idx / width))
                else {
                    return false;
                };
                let stale = self.vision_maps[p].visibility_at(x, y) == VisibilityState::Visible
                    && self
                        .map
                        .cell(x, y)
                        .is_ok_and(|c| !c.flags.contains(CellFlags::OCCUPIED_BY_STRUCTURE));
                if stale {
                    self.known_blocked_cells[p][idx] = false;
                    self.movement.recheck_routes[p] = true;
                }
                !stale
            });
            self.known_blocked_lists[p] = list;
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
    pub(crate) fn compute_entity_flee_destination_prevalidated(
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
        let away = if squared_distance(victim_pos, threat_pos) > (0, FLEE_COINCIDENCE_SQUARED) {
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

    /// Phase 8: Reap destroyed entities and emit death shatter visuals.
    pub fn reap_dead_and_emit_shatter(&mut self) {
        self.reap_transport_groups();
        self.update_match_state();
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

    /// Fraction of a foundation built, or of a building's front job done.
    fn published_progress(entity: &Entity) -> Option<f32> {
        let (remaining, total) = match (&entity.construction, entity.production_queue.front()) {
            (Some(record), _) => (record.remaining_ticks, record.total_ticks),
            (None, Some(job)) => (job.remaining_ticks, job.total_ticks),
            (None, None) => return None,
        };
        let done = total.saturating_sub(remaining);
        let fraction = u64::from(done) * (1 << 16) / u64::from(total.max(1));
        Some(SimScalar::from_raw(i64::try_from(fraction).unwrap_or(0) << 16).to_f32())
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
        output.match_result = self.match_state.result;
        for (slot, view) in output.players.iter_mut().enumerate() {
            let resources = &self.player_resources[slot];
            *view = PlayerStatusView {
                minerals: resources.minerals,
                gas: resources.gas,
                supply_used: resources.supply_used,
                supply_max: resources.supply_max,
                defeated: self.match_state.defeated & (1 << slot) != 0,
            };
        }
        output.resource_nodes.clear();
        output
            .resource_nodes
            .extend(
                self.map
                    .resources()
                    .iter()
                    .zip(0_u32..)
                    .map(|(node, index)| ResourceNodeView {
                        id: ResourceNodeId(index),
                        kind: node.kind,
                        position: [node.pos.x.to_f32(), node.pos.y.to_f32()],
                        remaining: node.remaining,
                    }),
            );

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
                build_or_queue_progress: Self::published_progress(entity),
                selected: false,
                cloaked: entity.is_cloaked,
                speed_boosted,
            });
        }

        output.combat_events.append(&mut self.pending_events);
    }
}
