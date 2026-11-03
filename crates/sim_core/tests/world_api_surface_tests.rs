//! Compile-time record of the public `SimWorld` API and the crate-root re-exports.
//!
//! # Responsibility
//! Pins the exact public surface of `sim_core::world` (every `pub` method of `SimWorld` with its
//! full signature, every `pub` field with its type, and the fields of `PlayerResources` and
//! `ActiveHazardZone`) and names every item `sim_core` re-exports at its root.
//!
//! # How it fits
//! Phase 9 restructures `world.rs` (WP1) and then changes this API on purpose (WP10 removes the
//! embedded-catalog constructors, WP11 makes per-player storage typed). A pure refactor must leave
//! this file untouched and compiling; a deliberate change edits it, so the diff of this file is
//! the exact record of how the public API moved. See
//! `docs/plans/2026-11-03-phase-9-structure-and-docs.md`, WP0.
//!
//! # Invariants
//! - Every method is coerced to a function pointer whose type spells out each parameter and the
//!   return type, so a changed signature is a compile error here, not a silent pass.
//! - The two plain-data structs are built with struct literals naming every field, so adding,
//!   removing or retyping a field fails to compile.
//! - The checks run at compile time; the single `#[test]` only makes the binary part of the suite.

use rand_chacha::ChaCha8Rng;
use sim_core::match_state::MatchState;
use sim_core::movement::{MovementInvariant, MovementMathError};
use sim_core::world::{ActiveHazardZone, PlayerResources, SimWorld};
use sim_core::{
    calculate_hit_damage, calculate_hit_damage_upgraded, calculate_splash_damage,
    calculate_splash_damage_upgraded, check_cluster_arrival, compute_concave_arc_steering,
    compute_push_displacement, compute_threat_priority, compute_unified_local_steering,
    compute_world_checksum, is_in_weapon_range, is_within_firing_arc, resolve_push_interaction,
    step_entity_weapon, ActionSet, ActiveBuff, ActiveProjectile, AttackRate, Cargo,
    DamageResolution, Entity, InvalidAttackRate, InvalidBuffAttackRate, MovementMemory,
    PushEntityParams, SimCoreError, SimResult, SimulationDiagnostic, SpatialHashGrid, SplashSpec,
    SteeringObstacle, TargetPriorityKey, TechTreeValidator, UnitArchetype, UnitCatalog, UnitTask,
    UpgradeCatalog, UpgradeDef, UpgradeId, WeaponPhase, WeaponSpec, WeaponTarget, GRID_HEIGHT,
    GRID_WIDTH, MIN_DAMAGE_CLAMP, RELEASE_RANGE_SLOP, SPATIAL_CELL_SIZE,
};
use sim_map::{Pathfinder, PlayerVisionMap, SimMap};
use sim_types::{
    CombatEventVisual, EntityId, PlayerId, PlayerUpgradeState, RenderSnapshot, SimScalar, SimVec2,
    TickBundle, UnitTypeId,
};
use std::collections::BTreeMap;
use std::marker::PhantomData;

/// `SimWorld::compute_flee_destination`: victim, threat, desired distance.
type FleeFromPoint =
    fn(&SimWorld, SimVec2, SimVec2, SimScalar) -> Result<Option<SimVec2>, MovementMathError>;

/// `SimWorld::compute_entity_flee_destination`: entity, threat position, distance.
type FleeEntity =
    fn(&SimWorld, EntityId, SimVec2, SimScalar) -> Result<Option<SimVec2>, MovementInvariant>;

/// Every public method of `SimWorld`, coerced to its full function-pointer type.
#[test]
fn sim_world_public_methods_keep_their_signatures() {
    // Construction and spawning.
    let _: fn(SimMap, u64, UnitCatalog, UpgradeCatalog) -> SimWorld = SimWorld::with_catalogs;
    let _: fn(&mut SimWorld, UnitTypeId, PlayerId, SimVec2) -> SimResult<EntityId> =
        SimWorld::spawn_unit;
    let _: fn(&mut SimWorld, UnitTypeId, PlayerId, u32, u32) -> SimResult<EntityId> =
        SimWorld::spawn_structure;

    // Upgrades, research and effective statistics.
    let _: fn(&SimWorld, PlayerId) -> &PlayerUpgradeState = SimWorld::player_upgrade_state;
    let _: fn(&mut SimWorld, PlayerId) -> &mut PlayerUpgradeState =
        SimWorld::player_upgrade_state_mut;
    let _: fn(&mut SimWorld, PlayerId, &str) -> SimResult<()> = SimWorld::research_upgrade;
    let _: fn(&SimWorld, PlayerId, &str) -> SimResult<()> = SimWorld::can_construct_structure;
    let _: fn(&SimWorld, PlayerId, &str) -> SimResult<()> = SimWorld::can_research_upgrade;
    let _: fn(&SimWorld, &UnitArchetype, PlayerId) -> Result<SimScalar, MovementMathError> =
        SimWorld::effective_max_speed;
    let _: fn(&SimWorld, &WeaponSpec, &str, PlayerId) -> SimScalar =
        SimWorld::effective_weapon_range;
    let _: fn(&SimWorld, &WeaponSpec, &str, PlayerId) -> u16 = SimWorld::effective_attack_period;
    let _: fn(&SimWorld, &UnitArchetype, PlayerId) -> SimScalar = SimWorld::effective_sight_radius;
    let _: fn(&SimWorld, &UnitArchetype, PlayerId, &str) -> bool = SimWorld::is_ability_unlocked;
    let _: fn(&SimWorld, &UnitArchetype, PlayerId) -> SimScalar = SimWorld::effective_max_hp;

    // The tick and its phases.
    let _: fn(&mut SimWorld, &TickBundle) -> RenderSnapshot = SimWorld::step;
    let _: fn(&mut SimWorld, &TickBundle, &mut RenderSnapshot) = SimWorld::step_into;
    let _: fn(&SimWorld) -> &[SimulationDiagnostic] = SimWorld::diagnostics;
    let _: fn(&mut SimWorld) -> std::vec::Drain<'_, SimulationDiagnostic> =
        SimWorld::drain_diagnostics;
    let _: fn(&SimWorld) -> u64 = SimWorld::last_checksum;
    let _: fn(&mut SimWorld, EntityId, u8, Option<SimVec2>, Option<EntityId>) -> bool =
        SimWorld::execute_ability;
    let _: fn(&mut SimWorld, &TickBundle) = SimWorld::process_commands;
    let _: fn(&mut SimWorld) = SimWorld::update_production_and_buffs;
    let _: fn(&mut SimWorld) = SimWorld::update_vision;
    let _: fn(&mut SimWorld, EntityId, EntityId) -> bool = SimWorld::load_unit_into_transport;
    let _: fn(&mut SimWorld, EntityId, SimVec2) -> bool = SimWorld::unload_unit_from_transport;
    let _: FleeFromPoint = SimWorld::compute_flee_destination;
    let _: FleeEntity = SimWorld::compute_entity_flee_destination;
    let _: fn(&mut SimWorld) = SimWorld::step_movement_and_steering;
    let _: fn(&SimWorld, SimVec2, SimVec2) -> bool = SimWorld::is_position_pathable_from;
    let _: fn(&SimMap, SimVec2, SimVec2) -> bool = SimWorld::is_position_pathable;
    let _: fn(&mut SimWorld) = SimWorld::reap_dead_and_emit_shatter;
    let _: fn(&SimWorld) -> u64 = SimWorld::compute_checksum;
    let _: fn(&mut SimWorld) -> RenderSnapshot = SimWorld::build_render_snapshot;
    let _: fn(&mut SimWorld, &mut RenderSnapshot) = SimWorld::write_render_snapshot;
}

/// Every public field of `SimWorld`, with its type.
#[test]
fn sim_world_public_fields_keep_their_types() {
    let _: fn(&SimWorld) -> &u32 = |w| &w.current_tick;
    let _: fn(&SimWorld) -> &ChaCha8Rng = |w| &w.rng;
    let _: fn(&SimWorld) -> &u64 = |w| &w.rng_seed;
    let _: fn(&SimWorld) -> &BTreeMap<EntityId, Entity> = |w| &w.entities;
    let _: fn(&SimWorld) -> &Vec<ActiveProjectile> = |w| &w.projectiles;
    let _: fn(&SimWorld) -> &u32 = |w| &w.next_entity_id;
    let _: fn(&SimWorld) -> &u32 = |w| &w.next_projectile_id;
    let _: fn(&SimWorld) -> &SimMap = |w| &w.map;
    let _: fn(&SimWorld) -> &Pathfinder = |w| &w.pathfinder;
    let _: fn(&SimWorld) -> &[PlayerVisionMap; 8] = |w| &w.vision_maps;
    let _: fn(&SimWorld) -> &UnitCatalog = |w| &w.catalog;
    let _: fn(&SimWorld) -> &UpgradeCatalog = |w| &w.upgrade_catalog;
    let _: fn(&SimWorld) -> &[PlayerUpgradeState; 8] = |w| &w.player_upgrades;
    let _: fn(&SimWorld) -> &SpatialHashGrid = |w| &w.spatial_hash;
    let _: fn(&SimWorld) -> &[PlayerResources; 8] = |w| &w.player_resources;
    let _: fn(&SimWorld) -> &u64 = |w| &w.last_checksum;
    let _: fn(&SimWorld) -> &Vec<CombatEventVisual> = |w| &w.pending_events;
    let _: fn(&SimWorld) -> &MatchState = |w| &w.match_state;
    let _: fn(&SimWorld) -> &Vec<ActiveHazardZone> = |w| &w.active_hazards;
    let _: fn(&SimWorld) -> &u32 = |w| &w.next_hazard_id;
    let _: fn(&SimWorld) -> &bool = |w| &w.vision_aware_pathfinding;
    let _: fn(&SimWorld) -> &[Vec<bool>; 8] = |w| &w.known_blocked_cells;
}

/// `PlayerResources` and `ActiveHazardZone` are built field by field, so their field sets and
/// types are pinned.
#[test]
fn plain_world_records_keep_their_fields() {
    let resources = PlayerResources {
        minerals: 0_u32,
        gas: 0_u32,
        supply_used: 0_u32,
        supply_max: 0_u32,
        supply_provided_total: 0_u32,
    };
    let hazard = ActiveHazardZone {
        id: 0_u32,
        owner: PlayerId(0),
        center: SimVec2::ZERO,
        radius: SimScalar::ZERO,
        delay_ticks: 0_u16,
        remaining_ticks: 0_u16,
        interval_ticks: 0_u16,
        ticks_until_next_interval: 0_u16,
        damage_per_interval: SimScalar::ZERO,
        targets_friendly: false,
    };
    assert_eq!(resources.minerals, 0);
    assert!(!hazard.targets_friendly);
}

// Every type re-exported at the crate root (and the three `world` types) must keep its name.
const _: PhantomData<SplashSpec> = PhantomData;
const _: PhantomData<UnitArchetype> = PhantomData;
const _: PhantomData<UnitCatalog> = PhantomData;
const _: PhantomData<WeaponSpec> = PhantomData;
const _: PhantomData<DamageResolution> = PhantomData;
const _: PhantomData<TargetPriorityKey> = PhantomData;
const _: PhantomData<WeaponTarget> = PhantomData;
const _: PhantomData<ActiveBuff> = PhantomData;
const _: PhantomData<ActiveProjectile> = PhantomData;
const _: PhantomData<Cargo> = PhantomData;
const _: PhantomData<Entity> = PhantomData;
const _: PhantomData<MovementMemory> = PhantomData;
const _: PhantomData<UnitTask> = PhantomData;
const _: PhantomData<WeaponPhase> = PhantomData;
const _: PhantomData<SimCoreError> = PhantomData;
const _: PhantomData<SimResult<()>> = PhantomData;
const _: PhantomData<ActionSet> = PhantomData;
const _: PhantomData<PushEntityParams> = PhantomData;
const _: PhantomData<SpatialHashGrid> = PhantomData;
const _: PhantomData<SteeringObstacle> = PhantomData;
const _: PhantomData<TechTreeValidator> = PhantomData;
const _: PhantomData<UpgradeCatalog> = PhantomData;
const _: PhantomData<UpgradeDef> = PhantomData;
const _: PhantomData<UpgradeId> = PhantomData;
const _: PhantomData<AttackRate> = PhantomData;
const _: PhantomData<InvalidAttackRate> = PhantomData;
const _: PhantomData<InvalidBuffAttackRate> = PhantomData;
const _: PhantomData<SimulationDiagnostic> = PhantomData;
const _: PhantomData<ActiveHazardZone> = PhantomData;
const _: PhantomData<PlayerResources> = PhantomData;
const _: PhantomData<SimWorld> = PhantomData;

/// Names every function and constant re-exported at the crate root, so removing or renaming one
/// fails to compile; the types are named by the `const _` items above.
#[test]
fn crate_root_reexports_exist() {
    let _ = (
        compute_world_checksum,
        calculate_hit_damage,
        calculate_hit_damage_upgraded,
        calculate_splash_damage,
        calculate_splash_damage_upgraded,
        compute_threat_priority,
        is_in_weapon_range,
        is_within_firing_arc,
        step_entity_weapon,
        check_cluster_arrival,
        compute_concave_arc_steering,
        compute_push_displacement,
        compute_unified_local_steering,
        resolve_push_interaction,
    );
    let _ = (
        MIN_DAMAGE_CLAMP,
        RELEASE_RANGE_SLOP,
        GRID_HEIGHT,
        GRID_WIDTH,
        SPATIAL_CELL_SIZE,
    );
}
