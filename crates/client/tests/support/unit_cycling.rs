//! Stages battles in which units arrive and leave the presented snapshot on their own clocks or
//! at a test's cadence (plan `2026-11-29-render-entity-pool` R-1, R-1v, R-2).
//!
//! # Responsibility
//! - [`stage_brood`]: the broodlings plan's B-19 stage. Every mobile unit of the battle is
//!   killed, and a Brood Lord analogue (the shipped unit with a strike magazine) attacks a
//!   durable, unarmed ground unit holding beside it at the map's centre, so its Broodlings are
//!   created at every impact and expire on their timed lives.
//! - [`stage_air_cycle`]: every mobile unit is killed, and an unarmed air unit with media of
//!   each player is killed and spawned again at a fixed cadence by [`AirCycle::advance`]; every
//!   other cycle a ground unit replaces the air one on the same tick, so render entities move
//!   between the air and ground layouts.
//!
//! # How tests use it
//! Include it with `#[path = "support/unit_cycling.rs"]` and call a stage on the session's
//! world (through the harness's `world_mut`) or on a world before its session is prepared. It is
//! setup: only the world and its catalog copy change, never a file.
//!
//! # Invariants
//! - Every unit is found by catalog property (strike magazine, mover type, media present),
//!   never by slug, so the stage follows the data.
//! - The lifetimes and cadences are read from the data: [`BroodStage::lifetime`] is the longest
//!   timed life the analogue's spawning weapons give.

#![allow(clippy::expect_used)]

use std::path::Path;

use sim_core::SimWorld;
use sim_types::{EntityId, MoverType, PlayerId, SimScalar, SimVec2, UnitOrder, UnitTypeId};

/// The B-19 stage after [`stage_brood`].
#[derive(Debug, Clone, Copy)]
pub struct BroodStage {
    /// The Brood Lord analogue.
    pub lord: EntityId,
    /// The durable, unarmed target it attacks.
    pub target: EntityId,
    /// The archetype its impacts create.
    pub spawned: UnitTypeId,
    /// The longest life of a created unit, in ticks: the timed life plus its random extra
    /// (`L_max` of the plan).
    pub lifetime: u32,
}

/// Kills every unit of `world` that can move, so no fight but the staged one happens. The units
/// die on the next tick, outside any measured window.
pub fn kill_mobile_units(world: &mut SimWorld) {
    let mobile: Vec<EntityId> = world
        .entities
        .values()
        .filter(|e| {
            world
                .catalog
                .get(e.unit_type_id)
                .is_ok_and(|a| a.max_speed > SimScalar::ZERO)
        })
        .map(|e| e.id)
        .collect();
    for id in mobile {
        if let Some(entity) = world.entities.get_mut(&id) {
            entity.hp = SimScalar::ZERO;
        }
    }
}

/// The map's centre, in simulation world units.
pub fn map_centre(world: &SimWorld) -> SimVec2 {
    SimVec2::from_i32(
        i32::try_from(world.map.width() / 2).expect("a small map"),
        i32::try_from(world.map.height() / 2).expect("a small map"),
    )
}

/// Removes the weapons of `kind` in `world`'s catalog copy.
fn disarm(world: &mut SimWorld, kind: UnitTypeId) {
    let arch = world.catalog.get_mut(kind).expect("the kind is in the catalog");
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
}

/// Stages B-19 on `world` (see the module docs) and returns what it placed.
pub fn stage_brood(world: &mut SimWorld) -> BroodStage {
    let (lord, spawned, lifetime, dummy) = {
        let lord = world
            .catalog
            .archetypes()
            .find(|a| a.capabilities.strike_magazine.is_some())
            .expect("a shipped unit with a strike magazine");
        let spawn = lord
            .ground_weapon
            .as_ref()
            .and_then(|w| w.impact_spawn)
            .expect("its weapon spawns");
        let life = spawn.timed_life.expect("its spawn has a timed life");
        let lifetime = u32::from(life.ticks) + u32::from(life.random_extra_ticks);
        let mut ground: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| a.mover_type == MoverType::Ground)
            .filter(|a| a.max_speed > SimScalar::ZERO && a.unit_type_id != spawn.unit)
            .filter(|a| a.capabilities.modes.is_empty() && a.harvester.is_none())
            .map(|a| a.unit_type_id)
            .collect();
        ground.sort();
        (lord.unit_type_id, spawn.unit, lifetime, ground[0])
    };
    kill_mobile_units(world);
    disarm(world, dummy);
    world.catalog.get_mut(dummy).expect("the dummy kind").max_hp = SimScalar::from_i32(1_000_000);
    let centre = map_centre(world);
    let attacker = world
        .spawn_unit(lord, PlayerId(0), centre)
        .expect("open ground at the centre");
    let target = world
        .spawn_unit(dummy, PlayerId(1), centre + SimVec2::from_i32(5, 0))
        .expect("open ground beside it");
    world
        .entities
        .get_mut(&attacker)
        .expect("the lord")
        .issue_order(UnitOrder::attack(target), false)
        .expect("an attack order");
    world
        .entities
        .get_mut(&target)
        .expect("the dummy")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("a hold order");
    BroodStage {
        lord: attacker,
        target,
        spawned,
        lifetime,
    }
}

/// Ticks between two cycles of [`AirCycle`].
pub const AIR_CYCLE_TICKS: u32 = 12;

/// The R-2 stage after [`stage_air_cycle`]: one cycling unit per player.
#[derive(Debug, Clone)]
pub struct AirCycle {
    /// The unarmed air kind.
    pub air: UnitTypeId,
    /// The unarmed ground kind that replaces it every other cycle.
    pub ground: UnitTypeId,
    /// Each player's cycling unit now, and where it is placed.
    pub units: Vec<(PlayerId, EntityId, SimVec2)>,
    /// Cycles run so far.
    pub cycles: u32,
}

impl AirCycle {
    /// Runs a cycle if `world`'s tick is a multiple of [`AIR_CYCLE_TICKS`] and none ran on it:
    /// each player's unit is killed and a unit of the next kind (air on even cycles, ground on
    /// odd ones) is spawned at its place on the same tick. Returns whether a cycle ran.
    pub fn advance(&mut self, world: &mut SimWorld, last_tick: &mut u32) -> bool {
        let tick = world.current_tick;
        if tick == *last_tick || tick % AIR_CYCLE_TICKS != 0 {
            return false;
        }
        *last_tick = tick;
        self.cycles += 1;
        let kind = if self.cycles % 2 == 0 {
            self.air
        } else {
            self.ground
        };
        for (owner, id, at) in &mut self.units {
            if let Some(entity) = world.entities.get_mut(id) {
                entity.hp = SimScalar::ZERO;
            }
            *id = world.spawn_unit(kind, *owner, *at).expect("open ground");
        }
        true
    }
}

/// Stages R-2 on `world`: every mobile unit is killed and one unarmed air unit with media per
/// player is spawned near the map's centre, three world units apart. `assets` is the shipped
/// asset root, read to find units with media.
pub fn stage_air_cycle(world: &mut SimWorld, assets: &Path) -> AirCycle {
    let presentable = |slug: &str| {
        assets
            .join("textures")
            .join(format!("{slug}.flipbook.ron"))
            .is_file()
    };
    let pick = |mover: MoverType| {
        let mut kinds: Vec<UnitTypeId> = world
            .catalog
            .archetypes()
            .filter(|a| a.mover_type == mover && a.max_speed > SimScalar::ZERO)
            .filter(|a| a.capabilities.modes.is_empty() && a.harvester.is_none())
            .filter(|a| a.capabilities.hangar.is_none() && a.capabilities.strike_magazine.is_none())
            .filter(|a| presentable(&a.slug))
            .map(|a| a.unit_type_id)
            .collect();
        kinds.sort();
        kinds.first().copied()
    };
    let air = pick(MoverType::Air).expect("a shipped air unit with media");
    let ground = pick(MoverType::Ground).expect("a shipped ground unit with media");
    kill_mobile_units(world);
    disarm(world, air);
    disarm(world, ground);
    let centre = map_centre(world);
    let players = [PlayerId(0), PlayerId(1)];
    let units = players
        .iter()
        .zip([-3, 3])
        .map(|(&owner, dx)| {
            let at = centre + SimVec2::from_i32(dx, 0);
            let id = world.spawn_unit(air, owner, at).expect("open ground");
            (owner, id, at)
        })
        .collect();
    AirCycle {
        air,
        ground,
        units,
        cycles: 0,
    }
}
