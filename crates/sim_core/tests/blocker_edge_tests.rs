//! A unit standing in a sight blocker's edge is hidden from ground sight outside (B-57).
//!
//! # Responsibility
//! Pins plan `2026-11-23-vision-and-detection` P-8 end to end: on a scratch map with a 3x3 sight
//! blocker, an enemy standing in the blocker's edge cell is not acquired by an armed ground unit
//! outside, whose player has only ground sight, and is acquired once an air unit of that player
//! sees the cell (Blizzard, *Game Guide: Map Elements*: "Units inside obstructing terrain can't
//! be seen by units or structures on the outside").
//!
//! # How it fits
//! The map rule lives in `sim_map::PlayerVisionMap::cast` and is tested there
//! (`vision_tests.rs`); this file checks that target acquisition reads it.
//!
//! # Invariants
//! - No unit is named: archetypes are found by property (charter §3.3).
//! - The map is built through `sim_map`; every radius and range is the catalog's.

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The blocker's cells: x 20–22, y 14–16. The target stands in its west edge cell (20, 15).
const EDGE: (i32, i32) = (20, 15);
/// The attacker stands four cells west of the edge cell.
const ATTACKER: (i32, i32) = (16, 15);

fn centre(cell: (i32, i32)) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(f64::from(cell.0) + 0.5),
        SimScalar::from_num(f64::from(cell.1) + 0.5),
    )
}

fn blocker_map() -> Result<GameMap, sim_map::MapError> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 14..17 {
        for x in 20..23 {
            map.cell_mut(x, y)?.flags.insert(CellFlags::LOS_BLOCKER);
        }
    }
    Ok(map)
}

/// Registers a copy of `arch` without weapons under the next free identity.
fn unarmed_copy(world: &mut SimWorld, arch: &UnitArchetype, slug: &str) -> UnitTypeId {
    let id = UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let mut copy = arch.clone();
    copy.unit_type_id = id;
    slug.clone_into(&mut copy.slug);
    copy.weapon = None;
    copy.ground_weapon = None;
    copy.air_weapon = None;
    copy.detection_radius = None;
    world.catalog.register(copy);
    id
}

fn step(world: &mut SimWorld, ticks: std::ops::Range<u32>) {
    for tick in ticks {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
    }
}

/// P-8 end to end.
#[test]
fn a_unit_in_a_blocker_edge_is_not_acquired_from_outside() -> TestResult {
    let mut world = sim_fixtures::world(blocker_map()?, 0x57);
    // An armed mobile ground archetype, not a detector, whose ground weapon and sight reach the
    // edge cell from four cells away.
    let attacker_arch = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
                && arch.sight_radius > SimScalar::from_i32(5)
                && arch
                    .weapon_against(MoverType::Ground, None)
                    .is_some_and(|w| w.max_range >= SimScalar::from_i32(4) && w.splash.is_none())
        })
        .cloned()
        .ok_or("an armed ground archetype with reach 4")?;
    let air_arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.mover_type == MoverType::Air && arch.max_speed > SimScalar::ZERO)
        .cloned()
        .ok_or("an air archetype")?;
    let dummy = unarmed_copy(&mut world, &attacker_arch, "scratch_unarmed_target");
    let spotter = unarmed_copy(&mut world, &air_arch, "scratch_unarmed_spotter");
    let attacker = world.spawn_unit(attacker_arch.unit_type_id, PlayerId(0), centre(ATTACKER))?;
    let target = world.spawn_unit(dummy, PlayerId(1), centre(EDGE))?;
    for id in [attacker, target] {
        world
            .entities
            .get_mut(&id)
            .ok_or("spawned")?
            .issue_order(UnitOrder::HoldPosition, false)
            .map_err(|error| format!("{error:?}"))?;
    }
    let vitals =
        |world: &SimWorld, id: EntityId| world.entities.get(&id).map(|e| (e.hp, e.shields));
    let full = vitals(&world, target);
    step(&mut world, 0..40);
    assert_eq!(
        world.entities[&attacker].current_target, None,
        "ground sight from outside does not reveal the edge cell"
    );
    assert_eq!(
        vitals(&world, target),
        full,
        "the hidden unit is not struck"
    );

    world.spawn_unit(spotter, PlayerId(0), centre((18, 15)))?;
    step(&mut world, 40..80);
    assert_eq!(
        world.entities[&attacker].current_target,
        Some(target),
        "an air unit of the same player sees into the blocker"
    );
    Ok(())
}
