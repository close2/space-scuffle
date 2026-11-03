//! The fixed battle sandbox: two small armies and two bases on a flat arena.
//!
//! # Responsibility
//! Builds one deterministic scene from the shipped catalogs, choosing each squad's archetype by
//! role and capability rather than by name (charter §3.3).
//!
//! # How it fits
//! Formerly `SimWorld::new_default_battle_sandbox` in `sim_core`. Tests of the simulation, the
//! client pipeline and the diagnostics use it as a small, busy world. The game itself opens
//! `assets/scenarios/default_battle.ron` from disk instead.
//!
//! # Invariants
//! - The scene depends only on the shipped catalogs, the map and the fixed seed, so it is
//!   identical on every platform and every run.
//! - Every spawn is checked: a scene that cannot be built whole is an error, never a world with
//!   pieces silently missing.
//! - The returned world's `last_checksum` describes the spawned state.

use sim_core::{SimCoreError, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell, SimMap};
use sim_types::{AttributeFlags, PlayerId, ShapeKind, SimScalar, SimVec2};

/// Edge length, in cells, of the default sandbox arena.
const ARENA_CELLS: u32 = 64;

/// RNG seed of the sandbox world.
const SANDBOX_SEED: u64 = 0x5EED_BEEF;

/// Why the sandbox could not be built.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// The catalog has no archetype that can play a role the scene needs.
    #[error("no catalog archetype can play the sandbox role `{role}`")]
    MissingRole {
        /// The role, for example `light ranged unit`.
        role: &'static str,
    },
    /// The simulation refused to place one of the scene's entities.
    #[error("cannot spawn the sandbox {role}: {source}")]
    Spawn {
        /// The role of the entity that could not be placed.
        role: &'static str,
        /// The simulation's reason.
        #[source]
        source: SimCoreError,
    },
    /// The arena map could not be created.
    #[error("cannot create the sandbox arena: {0}")]
    Map(#[from] sim_map::MapError),
}

/// Builds the default battle sandbox on a flat 64×64 arena.
///
/// Spawns, choosing archetypes by role (see [`default_battle_sandbox_on`]):
/// - player 0: ten light ranged units at x = 15 and two armoured units at x = 12, around y = 32;
/// - player 1: eight melee units at x = 45 and two turreted units at x = 48, around y = 32;
/// - player 0's base: command, supply and production structures at cells (5, 5), (12, 5), (16, 5);
/// - player 1's base: the same at cells (54, 54), (48, 54), (42, 54).
///
/// # Errors
/// Returns [`SandboxError`] if the arena cannot be created, a role has no archetype, or an
/// entity cannot be spawned.
///
/// # Panics
/// Panics if the shipped catalogs are invalid (see [`crate::shipped_catalogs`]).
pub fn default_battle_sandbox() -> Result<SimWorld, SandboxError> {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    default_battle_sandbox_on(GameMap::new(ARENA_CELLS, ARENA_CELLS, default_cell)?)
}

/// Builds the default battle sandbox's scene on `map`, with the shipped catalogs.
///
/// Positions are the fixed world coordinates of [`default_battle_sandbox`]; a map too small to
/// hold them yields [`SandboxError::Spawn`].
///
/// # Errors
/// Returns [`SandboxError`] if a role has no archetype or an entity cannot be spawned.
///
/// # Panics
/// Panics if the shipped catalogs are invalid (see [`crate::shipped_catalogs`]).
pub fn default_battle_sandbox_on(map: SimMap) -> Result<SimWorld, SandboxError> {
    let mut world = crate::world(map, SANDBOX_SEED);
    let roles = Roles::resolve(&world)?;

    let blue = PlayerId(0);
    let red = PlayerId(1);
    // Columns of units: (role, archetype, owner, x, count, spacing, first y offset).
    let columns = [
        ("light ranged unit", roles.light_ranged, blue, 15, 10, 2, -9),
        ("armoured unit", roles.heavy_armored, blue, 12, 2, 4, -2),
        ("melee unit", roles.melee_assault, red, 45, 8, 2, -7),
        ("turreted unit", roles.siege_turret, red, 48, 2, 4, -2),
    ];
    for (role, archetype, owner, x, count, spacing, first) in columns {
        for i in 0..count {
            let y = SimScalar::from_num(32) + SimScalar::from_num(i * spacing + first);
            let pos = SimVec2::new(SimScalar::from_num(x), y);
            world
                .spawn_unit(archetype, owner, pos)
                .map_err(|source| SandboxError::Spawn { role, source })?;
        }
    }

    let structures = [
        ("command structure", roles.command, blue, 5, 5),
        ("supply structure", roles.supply, blue, 12, 5),
        ("production structure", roles.production, blue, 16, 5),
        ("command structure", roles.command, red, 54, 54),
        ("supply structure", roles.supply, red, 48, 54),
        ("production structure", roles.production, red, 42, 54),
    ];
    for (role, archetype, owner, cell_x, cell_y) in structures {
        world
            .spawn_structure(archetype, owner, cell_x, cell_y)
            .map_err(|source| SandboxError::Spawn { role, source })?;
    }

    world.last_checksum = world.compute_checksum();
    Ok(world)
}

/// The archetype chosen for each role of the scene.
struct Roles {
    light_ranged: sim_types::UnitTypeId,
    heavy_armored: sim_types::UnitTypeId,
    melee_assault: sim_types::UnitTypeId,
    siege_turret: sim_types::UnitTypeId,
    command: sim_types::UnitTypeId,
    supply: sim_types::UnitTypeId,
    production: sim_types::UnitTypeId,
}

impl Roles {
    /// Chooses every role's archetype from `world`'s catalog, by attributes and capabilities.
    ///
    /// The light ranged unit and the command structure are required; every other role falls
    /// back to one of them when no archetype fits, exactly as the original sandbox did.
    fn resolve(world: &SimWorld) -> Result<Self, SandboxError> {
        let is_dome =
            |a: &sim_core::UnitArchetype| matches!(a.shape, ShapeKind::BuildingDome { .. });
        let mobile = || world.catalog.archetypes().filter(|a| !is_dome(a));
        let domes = || world.catalog.archetypes().filter(|a| is_dome(a));

        let light_ranged = mobile()
            .find(|a| {
                a.attributes.contains(AttributeFlags::LIGHT)
                    && a.weapon
                        .as_ref()
                        .is_some_and(|w| w.max_range > SimScalar::from_num(1))
            })
            .or_else(|| mobile().next())
            .map(|a| a.unit_type_id)
            .ok_or(SandboxError::MissingRole {
                role: "light ranged unit",
            })?;
        let heavy_armored = mobile()
            .find(|a| {
                a.attributes.contains(AttributeFlags::ARMORED)
                    && !a.weapon.as_ref().is_some_and(|w| w.has_turret)
            })
            .map_or(light_ranged, |a| a.unit_type_id);
        let melee_assault = mobile()
            .find(|a| {
                a.capabilities.harvester.is_none()
                    && a.weapon
                        .as_ref()
                        .is_some_and(|w| w.max_range <= SimScalar::from_num(2))
            })
            .map_or(light_ranged, |a| a.unit_type_id);
        let siege_turret = mobile()
            .find(|a| a.weapon.as_ref().is_some_and(|w| w.has_turret))
            .map_or(heavy_armored, |a| a.unit_type_id);

        let command = domes()
            .find(|a| a.is_resource_dropoff)
            .or_else(|| domes().next())
            .map(|a| a.unit_type_id)
            .ok_or(SandboxError::MissingRole {
                role: "command structure",
            })?;
        let supply = domes()
            .find(|a| a.supply_provided > 0 && !a.is_resource_dropoff)
            .map_or(command, |a| a.unit_type_id);
        let production = domes()
            .find(|a| !a.is_resource_dropoff && a.supply_provided == 0 && a.weapon.is_none())
            .map_or(command, |a| a.unit_type_id);

        Ok(Self {
            light_ranged,
            heavy_armored,
            melee_assault,
            siege_turret,
            command,
            supply,
            production,
        })
    }
}
