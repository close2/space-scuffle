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
//!
//! ## Module layout
//! This file owns the [`SimWorld`] state, its construction, spawning and the tick sequence.
//! Each phase that is more than a call lives in a private child module with its own
//! `impl SimWorld` block: `commands` (phase 1), `upkeep` (phase 2), `vision` (phase 3),
//! `snapshot` (phase 10), plus `abilities` (casting and hazard zones), `upgrade_stats`
//! (research and effective stats) and `sandbox` (the Visual Lab scene). Children see the
//! struct's private fields, so splitting the file widened no field's visibility.

mod abilities;
mod commands;
mod sandbox;
mod snapshot;
mod upgrade_stats;
mod upkeep;
mod vision;

pub use abilities::ActiveHazardZone;
use vision::AppliedSight;

use crate::catalog::UnitCatalog;
use crate::checksum::compute_simulation_checksum;
use crate::combat::TargetPriorityKey;
use crate::entity::{ActiveProjectile, Entity};
use crate::error::{SimCoreError, SimResult};
use crate::steering::{SpatialHashGrid, SPATIAL_CELL_SIZE};
use crate::upgrades::UpgradeCatalog;
use crate::weapon_timing::SimulationDiagnostic;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{Pathfinder, PlayerVisionMap, SimMap};
use sim_types::{
    CombatEventVisual, EntityId, PlayerId, PlayerUpgradeState, RenderSnapshot, ShapeKind,
    SimScalar, SimVec2, TickBundle, UnitTypeId,
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
    // =========================================================================
    // 10-Phase Deterministic Pipeline Implementation
    // =========================================================================
    /// Phase5: Prepare and atomically commit each actor's checked movement operation.
    /// Malformed state emits diagnostics without partial pose, route or order edits.
    /// Direct phase callers must establish `validate_transport_links` beforehand;
    /// normal `step_into` establishes this precondition once for its phase sequence.
    pub fn step_movement_and_steering(&mut self) {
        self.advance_checked_movement();
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
}
