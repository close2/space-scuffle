//! # Integration Test: an attack ordered on one's own unit is carried out
//!
//! Plan `2026-11-24-attack-orders` P-4 (B-175, §2.2 rules 5 to 7). 5.0.14's attack ability
//! leaves out own and allied units only for automatic acquisition and the smart click (`core`
//! AbilData:117-119), and ordinary weapons accept own units (`core` WeaponData:3-31), so an
//! explicit attack on one's own unit attacks it. `SimWorld::issue_plain_order` admits an
//! `AttackTarget` by `Entity::can_attack`, which reads no owner. The victim neither answers nor
//! calls for help: acquisition and call-for-help leave out the attacker's own side.
//!
//! Every test drives the shipped catalog through `SimWorld::spawn_unit_facing` and
//! `SimWorld::step`, with the random firing delays removed so timings are exact. Archetypes are
//! found by property; no slug is written here (charter C5).

#![allow(clippy::expect_used, clippy::panic)]

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The owner of every unit in these tests.
const ME: PlayerId = PlayerId(0);

/// A 48 × 48 flat world on the shipped catalog without random firing delays.
fn flat_world() -> SimWorld {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat 48x48 map is valid");
    let mut world = sim_fixtures::world(map, 0xa77_ac4);
    sim_fixtures::without_random_delays(&mut world.catalog);
    world
}

/// A mobile ground archetype without modes or harvesting whose ground weapon hits at once: no
/// projectile, no splash, no burst, not melee and not suicidal, so its first strike lands at
/// its damage point.
fn plain_gunner(world: &SimWorld) -> Result<UnitArchetype, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default()
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && arch.capabilities.harvester.is_none()
                && arch
                    .weapon_against(MoverType::Ground, None)
                    .is_some_and(|weapon| {
                        weapon.projectile_speed.is_none()
                            && weapon.splash.is_none()
                            && weapon.attacks_count == 1
                            && !weapon.melee
                            && !weapon.suicide_on_attack
                            && weapon.max_range >= SimScalar::from_num(3)
                    })
        })
        .ok_or("a mobile ground unit with an instant single-strike ranged weapon")?
        .clone())
}

/// Steps `world` one tick with `commands`.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

/// The life and shields of `unit`.
fn vitals(world: &SimWorld, unit: EntityId) -> SimScalar {
    world.entities[&unit].hp + world.entities[&unit].shields
}

/// Orders `attacker` to attack `victim`, both of [`ME`], then steps until the victim has lost
/// life or `ticks` have passed. Returns the tick count it took, and checks every tick that
/// neither the victim nor `bystander` (another own unit) acquired anything.
fn attack_own(
    world: &mut SimWorld,
    attacker: EntityId,
    victim: EntityId,
    bystander: EntityId,
    ticks: u32,
) -> Option<u32> {
    let full = vitals(world, victim);
    step_with(
        world,
        vec![PlayerCommand {
            player_id: ME,
            entities: vec![attacker],
            order: UnitOrder::attack(victim),
            queue: false,
        }],
    );
    assert_eq!(
        world.entities[&attacker].order_queue.front(),
        Some(&UnitOrder::attack(victim)),
        "the attack on an own unit is admitted"
    );
    for tick in 1..=ticks {
        for quiet in [victim, bystander] {
            assert_eq!(
                world.entities[&quiet].current_target, None,
                "an own unit attacked by its own side neither answers nor is called to help"
            );
        }
        if vitals(world, victim) < full {
            return Some(tick);
        }
        step_with(world, Vec::new());
    }
    None
}

/// P-4, in reach: an armed unit ordered to attack an own unit standing in its range, facing it,
/// damages it within one weapon period plus its damage point; the victim and an own bystander
/// next to it acquire nothing.
#[test]
fn an_armed_unit_attacks_an_own_unit_in_reach() -> TestResult {
    let mut world = flat_world();
    let gunner = plain_gunner(&world)?;
    let weapon = gunner
        .weapon_against(MoverType::Ground, None)
        .ok_or("its ground weapon")?
        .clone();
    let gap = gunner.collision_radius * SimScalar::from_num(2)
        + weapon.max_range / SimScalar::from_num(2);
    let origin = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(20));
    let attacker = world.spawn_unit_facing(gunner.unit_type_id, ME, origin, SimAngle::ZERO)?;
    let victim = world.spawn_unit_facing(
        gunner.unit_type_id,
        ME,
        origin + SimVec2::new(gap, SimScalar::ZERO),
        SimAngle::ZERO,
    )?;
    let bystander = world.spawn_unit_facing(
        gunner.unit_type_id,
        ME,
        origin + SimVec2::new(gap, SimScalar::from_num(3)),
        SimAngle::ZERO,
    )?;
    let bound = u32::from(weapon.period_ticks) + u32::from(weapon.damage_point_ticks);
    let took = attack_own(&mut world, attacker, victim, bystander, bound);
    assert!(
        took.is_some(),
        "{}: the own unit was struck within {bound} ticks",
        gunner.slug
    );
    Ok(())
}

/// P-4, out of reach: a mobile attacker ordered to attack an own unit beyond its range closes in
/// and strikes it, and the victim and an own bystander acquire nothing.
#[test]
fn a_mobile_attacker_closes_in_on_an_own_unit_out_of_reach() -> TestResult {
    let mut world = flat_world();
    let gunner = plain_gunner(&world)?;
    let weapon = gunner
        .weapon_against(MoverType::Ground, None)
        .ok_or("its ground weapon")?
        .clone();
    let gap = gunner.collision_radius * SimScalar::from_num(2)
        + weapon.max_range
        + SimScalar::from_num(8);
    let origin = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(20));
    let attacker = world.spawn_unit_facing(gunner.unit_type_id, ME, origin, SimAngle::ZERO)?;
    let victim = world.spawn_unit_facing(
        gunner.unit_type_id,
        ME,
        origin + SimVec2::new(gap, SimScalar::ZERO),
        SimAngle::ZERO,
    )?;
    let bystander = world.spawn_unit_facing(
        gunner.unit_type_id,
        ME,
        origin + SimVec2::new(gap, SimScalar::from_num(3)),
        SimAngle::ZERO,
    )?;
    let start = world.entities[&attacker].position;
    let took = attack_own(&mut world, attacker, victim, bystander, 400);
    assert!(took.is_some(), "{}: the own unit was struck", gunner.slug);
    assert!(
        world.entities[&attacker].position.distance(start) > SimScalar::from_num(4),
        "the attacker closed in"
    );
    Ok(())
}
