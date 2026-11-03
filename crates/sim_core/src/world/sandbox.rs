//! The fixed battle sandbox the client Visual Lab opens with.
//!
//! # Responsibility
//! Builds one deterministic 64x64 scene from the embedded starter catalog, choosing each squad's
//! archetype by role and capability rather than by name (charter §3.3).
//!
//! # How it fits
//! A constructor on [`SimWorld`], kept apart from the authoritative tick so `world.rs` holds only
//! the world's state and pipeline. Phase 9 WP10 moves it to the test-only fixtures crate.
//!
//! # Invariants
//! - The scene depends only on the embedded catalog and the fixed seed, so it is identical on
//!   every platform and every run.
//! - The returned world's `last_checksum` describes the spawned state.

use crate::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{AttributeFlags, PlayerId, ShapeKind, SimScalar, SimVec2};

impl SimWorld {
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
}
