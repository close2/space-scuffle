//! # Integration Test: Turrets Under a Move Order
//!
//! A turret is a mount: it aims its weapon independently of the hull, and nothing more (plan
//! `2026-11-23-turret-wind-up`, B-90, B-45). Whether the weapon may fire under an order that is no
//! attack is its data, as in `StarCraft` II 5.0.14: every weapon keeps `OnlyFireWhileInAttackOrder`
//! unless its data clears it (`fires_outside_attack_order`), and only a `Moving` weapon fires while
//! its unit's order walks it (`allowed_movement`).
//!
//! These tests play the shipped catalog. They find their units by property, never by name:
//! - a turreted ground unit whose weapon keeps both defaults (the Siege Tank's or the Hellion's
//!   analogue), ordered to move away from an enemy it had as its target, translates exactly as it
//!   would with no enemy there and never starts a swing;
//! - a turreted air unit whose weapon is `Moving` and fires outside attack orders (the Phoenix's
//!   analogue), ordered to move past an enemy air unit, turns its turret independently of its
//!   hull and fires while it moves, its hull keeping its heading and translating on every tick,
//!   its wind-up included.
//!
//! A wind-up holds its unit unless the weapon of the swing is `Moving` (plan D-6, WP3), whatever
//! the mount: in a chase the turreted ground unit stands for each wind-up, and a hull weapon that
//! is `Moving` (the Void Ray's analogue) keeps chasing through its wind-up inside an attack order.
//!
//! Between shots a mount faces its target (5.0.14 `FaceTargetWhileInCooldown`): a turret at its
//! own rate (plan `2026-11-24-fire-rules` D-3), and the hull of a hull weapon on Hold Position at
//! the stationary rate (plan `2026-11-25-wind-up-and-hold-facing` D-4, B-199).
//!
//! Positions are world units; angles are `SimAngle` binary angles.

use sim_core::catalog::UnitArchetype;
use sim_core::{SimWorld, WeaponPhase, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AllowedMovement, EntityId, MoverType, PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2,
    Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The weapons in an archetype's own slots.
fn weapons(archetype: &UnitArchetype) -> impl Iterator<Item = &WeaponSpec> {
    [
        archetype.ground_weapon.as_ref(),
        archetype.air_weapon.as_ref(),
        archetype.weapon.as_ref(),
    ]
    .into_iter()
    .flatten()
}

/// The shipped archetype of lowest type id on `mover` that moves and has a weapon `pick` chooses.
fn find(
    world: &SimWorld,
    mover: MoverType,
    pick: impl Fn(&WeaponSpec) -> bool,
) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == mover
                && archetype.max_speed > SimScalar::ZERO
                && weapons(archetype).any(&pick)
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("no shipped archetype has the picked property")?
        .unit_type_id)
}

/// A world on a flat 64 × 64 map with the shipped catalog.
fn shipped_world() -> Result<SimWorld, Box<dyn std::error::Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    Ok(sim_fixtures::world(map, 0x9876_5432))
}

fn point(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y))
}

/// Steps `world` once at `tick` with `commands`.
fn step(world: &mut SimWorld, tick: u32, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
}

/// One player's order to one unit.
fn order(player: u8, unit: EntityId, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(player),
        entities: vec![unit],
        order,
        queue: false,
    }
}

/// A unit of `unit_type` at (20, 20) whose hull and turret face south (+y), in `world`.
fn spawn_facing_south(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let unit = world.spawn_unit(unit_type, PlayerId(0), point(20, 20))?;
    let entity = world.entities.get_mut(&unit).ok_or("unit")?;
    entity.heading = SimAngle::DEG_90;
    entity.turret_angle = SimAngle::DEG_90;
    Ok(unit)
}

/// B-90: a turret whose weapon needs an attack order (`OnlyFireWhileInAttackOrder`, the default)
/// holds fire under a move away from the enemy it had as its target. Its hull translates every
/// tick exactly as in a world without the enemy, its order stays the move, and it never winds up.
#[test]
fn a_turret_that_needs_an_attack_order_holds_fire_while_retreating() -> TestResult {
    let mut world = shipped_world()?;
    let unit_type = find(&world, MoverType::Ground, |weapon| {
        weapon.has_turret
            && !weapon.fires_outside_attack_order
            && weapon.allowed_movement == AllowedMovement::Stopped
    })?;
    let mut reference = shipped_world()?;
    let tank = spawn_facing_south(&mut world, unit_type)?;
    let lone = spawn_facing_south(&mut reference, unit_type)?;
    // The enemy 5 north, in reach, holding so that it never pushes the tank.
    let enemy = world.spawn_unit(unit_type, PlayerId(1), point(20, 15))?;
    world.entities.get_mut(&tank).ok_or("tank")?.current_target = Some(enemy);
    let away = UnitOrder::Move {
        target: point(20, 30),
    };
    step(
        &mut world,
        0,
        vec![
            order(0, tank, away.clone()),
            order(1, enemy, UnitOrder::HoldPosition),
        ],
    );
    step(&mut reference, 0, vec![order(0, lone, away.clone())]);
    let enemy_life = |world: &SimWorld| world.entities.get(&enemy).map(|e| (e.hp, e.shields));
    let unharmed = enemy_life(&world);
    let mut last = point(20, 20);
    for tick in 1..20 {
        step(&mut world, tick, Vec::new());
        step(&mut reference, tick, Vec::new());
        let unit = world.entities.get(&tank).ok_or("tank")?;
        let alone = reference.entities.get(&lone).ok_or("reference")?;
        assert_eq!(
            unit.position, alone.position,
            "tick {tick}: the hull translates as it would with no enemy"
        );
        assert_ne!(
            unit.position, last,
            "tick {tick}: the hull moves every tick"
        );
        last = unit.position;
        assert_eq!(
            unit.heading,
            SimAngle::DEG_90,
            "tick {tick}: the hull faces its path"
        );
        assert_eq!(
            unit.order_queue.front(),
            Some(&away),
            "tick {tick}: the move stays its order"
        );
        assert!(
            !matches!(
                unit.weapon_phase,
                WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
            ),
            "tick {tick}: no swing outside an attack order"
        );
    }
    assert_eq!(
        enemy_life(&world),
        unharmed,
        "the enemy took no damage from the retreating turret"
    );
    Ok(())
}

/// B-45: a turret whose weapon is `Moving` and fires outside attack orders (the Phoenix's
/// analogue) tracks an enemy air unit it passes under a move and fires at it, its turret turning
/// independently of the hull, while its hull keeps its heading and translates on every tick of
/// the pass, its wind-ups included (plan `2026-11-23-turret-wind-up` P-6, D-6, Amendment 1).
#[test]
fn a_turret_that_fires_while_moving_tracks_and_fires_while_retreating() -> TestResult {
    let mut world = shipped_world()?;
    let free = |weapon: &WeaponSpec| {
        weapon.has_turret
            && weapon.fires_outside_attack_order
            && weapon.allowed_movement == AllowedMovement::Moving
    };
    let unit_type = find(&world, MoverType::Air, free)?;
    let enemy_type = find(&world, MoverType::Air, |weapon| !free(weapon))?;
    let mut reference = shipped_world()?;
    let flier = spawn_facing_south(&mut world, unit_type)?;
    let lone = spawn_facing_south(&mut reference, unit_type)?;
    // The enemy behind and beside the path, holding so that it never chases or pushes.
    let enemy = world.spawn_unit(enemy_type, PlayerId(1), point(22, 18))?;
    let away = UnitOrder::Move {
        target: point(20, 40),
    };
    step(
        &mut world,
        0,
        vec![
            order(0, flier, away.clone()),
            order(1, enemy, UnitOrder::HoldPosition),
        ],
    );
    step(&mut reference, 0, vec![order(0, lone, away.clone())]);
    let enemy_life = |world: &SimWorld| world.entities.get(&enemy).map(|e| (e.hp, e.shields));
    let unharmed = enemy_life(&world);
    let (mut fired, mut turned, mut wound_up) = (false, false, false);
    let mut last = point(20, 20);
    for tick in 1..25 {
        step(&mut world, tick, Vec::new());
        step(&mut reference, tick, Vec::new());
        let unit = world.entities.get(&flier).ok_or("flier")?;
        let alone = reference.entities.get(&lone).ok_or("reference")?;
        if tick == 1 {
            assert_eq!(
                unit.position, alone.position,
                "the hull sets off as it would with no enemy"
            );
        }
        assert_ne!(
            unit.position, last,
            "tick {tick}: the hull moves on every tick, its wind-ups included"
        );
        wound_up |= matches!(unit.weapon_phase, WeaponPhase::Windup { .. });
        last = unit.position;
        assert_eq!(
            unit.heading,
            SimAngle::DEG_90,
            "tick {tick}: the hull faces its path"
        );
        assert!(
            matches!(unit.order_queue.front(), Some(UnitOrder::Move { .. })),
            "tick {tick}: the move stays its order"
        );
        turned |= unit.turret_angle != unit.heading;
        fired |= matches!(
            unit.weapon_phase,
            WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. } | WeaponPhase::Cooldown
        );
    }
    assert!(turned, "the turret turns independently of the hull");
    assert!(fired, "the weapon fires while the unit moves");
    assert!(wound_up, "premise: the pass includes a wind-up");
    assert_ne!(
        enemy_life(&world),
        unharmed,
        "the enemy takes damage from the passing turret"
    );
    Ok(())
}

/// One tick of a chase: whether the movement phase began it with the unit in its wind-up, and
/// whether the unit's position changed on it.
type ChaseFrame = (bool, bool);

/// A chase under an explicit attack order: a unit of `unit_type` at (10, 32), facing east,
/// attacks an enemy of its own kind 4 world units beyond its weapon's range further east, which
/// holds its position. On the tick the unit's first swing starts, the enemy is ordered to walk
/// east to (60, 32), out of the unit's reach, so the chase would move the unit during its
/// wind-up. Returns the frames of the `ticks` ticks after that order.
fn chase_frames(
    unit_type: UnitTypeId,
    ticks: u32,
) -> Result<Vec<ChaseFrame>, Box<dyn std::error::Error>> {
    let mut world = shipped_world()?;
    let archetype = world.catalog.get(unit_type)?;
    let range = archetype
        .weapon_against(archetype.mover_type, None)
        .ok_or("the unit's weapon against its own kind")?
        .max_range;
    let unit = world.spawn_unit(unit_type, PlayerId(0), point(10, 32))?;
    let start = SimVec2::new(range + SimScalar::from_i32(14), SimScalar::from_i32(32));
    let enemy = world.spawn_unit(unit_type, PlayerId(1), start)?;
    step(
        &mut world,
        0,
        vec![
            order(
                0,
                unit,
                UnitOrder::AttackTarget {
                    target: enemy,
                    origin: sim_types::AttackOrigin::Explicit,
                },
            ),
            order(1, enemy, UnitOrder::HoldPosition),
        ],
    );
    let in_windup = |world: &SimWorld| -> Result<(bool, SimVec2), Box<dyn std::error::Error>> {
        let entity = world.entities.get(&unit).ok_or("unit")?;
        Ok((
            matches!(entity.weapon_phase, WeaponPhase::Windup { .. }),
            entity.position,
        ))
    };
    let mut tick = 1;
    while !in_windup(&world)?.0 {
        if tick > 400 {
            return Err("premise: the unit starts a swing".into());
        }
        step(&mut world, tick, Vec::new());
        tick += 1;
    }
    let flee = order(
        1,
        enemy,
        UnitOrder::Move {
            target: point(60, 32),
        },
    );
    let mut frames = Vec::new();
    let mut commands = vec![flee];
    for tick in tick..tick + ticks {
        let (began_in_windup, before) = in_windup(&world)?;
        step(&mut world, tick, std::mem::take(&mut commands));
        frames.push((began_in_windup, in_windup(&world)?.1 != before));
    }
    Ok(frames)
}

/// B-90 (plan `2026-11-23-turret-wind-up` D-6, P-10): the turreted ground unit whose weapon keeps
/// the engine default (`Stopped`; the Siege Tank's or the Hellion's analogue) stands for every
/// tick of its wind-up while the enemy it attacks walks out of its reach, as a hull weapon's
/// unit does, and chases once the swing is over.
#[test]
fn a_turret_that_must_stand_to_fire_stands_for_its_wind_up_in_a_chase() -> TestResult {
    let world = shipped_world()?;
    let unit_type = find(&world, MoverType::Ground, |weapon| {
        weapon.has_turret
            && weapon.damage_point_ticks > 0
            && weapon.allowed_movement == AllowedMovement::Stopped
            && !weapon.fires_outside_attack_order
    })?;
    let frames = chase_frames(unit_type, 40)?;
    assert!(
        frames.first().is_some_and(|(windup, _)| *windup),
        "premise: the chase begins in a wind-up"
    );
    assert!(
        frames.iter().any(|(windup, moved)| !windup && *moved),
        "premise: the unit chases once its swing is over"
    );
    for (tick, (windup, moved)) in frames.iter().enumerate() {
        assert!(
            !(*windup && *moved),
            "tick {tick} of the chase: the wind-up holds the turreted unit"
        );
    }
    Ok(())
}

/// B-45 (plan `2026-11-23-turret-wind-up` D-6, P-10, S6): a hull weapon that fires while moving
/// (`Moving`, the Void Ray's analogue) does not hold its unit for its wind-up: inside an attack
/// order it chases the enemy walking out of its reach on the ticks of its wind-up.
#[test]
fn a_hull_weapon_that_fires_while_moving_chases_through_its_wind_up() -> TestResult {
    let world = shipped_world()?;
    let moving = |weapon: &WeaponSpec| {
        !weapon.has_turret
            && weapon.damage_point_ticks > 0
            && weapon.allowed_movement == AllowedMovement::Moving
    };
    let unit_type = find(&world, MoverType::Air, moving)
        .or_else(|_| find(&world, MoverType::Ground, moving))?;
    let frames = chase_frames(unit_type, 40)?;
    assert!(
        frames.first().is_some_and(|(windup, _)| *windup),
        "premise: the chase begins in a wind-up"
    );
    let wind_up_ticks = frames.iter().filter(|(windup, _)| *windup).count();
    let walked_in_wind_up = frames
        .iter()
        .filter(|(windup, moved)| *windup && *moved)
        .count();
    assert!(
        walked_in_wind_up > 0,
        "the unit chases through its wind-up ({wind_up_ticks} wind-up ticks, none walked)"
    );
    Ok(())
}

/// One tick of the between-shots scene: the weapon phase, the turret angle, the hull heading,
/// and the bearing to the enemy (`None` while it is not the unit's target).
type ShotFrame = (WeaponPhase, SimAngle, SimAngle, Option<SimAngle>);

/// The between-shots scene of B-197: a unit of `unit_type` at (20, 20) facing south, under a
/// player's attack (or on Hold Position when `hold`) on an enemy of the same kind 5 south of it,
/// which walks east across its front. Returns, for each of `ticks` ticks after the orders, the
/// unit's weapon phase, turret angle, hull heading and the bearing from it to the enemy (binary
/// angle), `None` while the enemy is not the unit's target (gone, or released out of reach).
fn between_shots_scene(
    unit_type: UnitTypeId,
    hold: bool,
    ticks: u32,
) -> Result<Vec<ShotFrame>, Box<dyn std::error::Error>> {
    let mut world = shipped_world()?;
    let unit = spawn_facing_south(&mut world, unit_type)?;
    let enemy = world.spawn_unit(unit_type, PlayerId(1), point(20, 25))?;
    let unit_order = if hold {
        UnitOrder::HoldPosition
    } else {
        UnitOrder::AttackTarget {
            target: enemy,
            origin: sim_types::AttackOrigin::Explicit,
        }
    };
    step(
        &mut world,
        0,
        vec![
            order(0, unit, unit_order),
            order(
                1,
                enemy,
                UnitOrder::Move {
                    target: point(60, 25),
                },
            ),
        ],
    );
    let mut frames = Vec::new();
    for tick in 1..=ticks {
        step(&mut world, tick, Vec::new());
        let entity = world.entities.get(&unit).ok_or("unit")?;
        let bearing = entity
            .current_target
            .filter(|&target| target == enemy)
            .and_then(|target| world.entities.get(&target))
            .map(|other| {
                let delta = other.position - entity.position;
                SimAngle::atan2(delta.y, delta.x)
            });
        frames.push((
            entity.weapon_phase.clone(),
            entity.turret_angle,
            entity.heading,
            bearing,
        ));
    }
    Ok(frames)
}

/// Whether `phase` is between shots: the follow-through or the cooldown of the last.
fn between_shots(phase: &WeaponPhase) -> bool {
    matches!(phase, WeaponPhase::Backswing { .. } | WeaponPhase::Cooldown)
}

/// P-12 of plan `2026-11-24-fire-rules` (B-197, D-3; 5.0.14 `FaceTargetWhileInCooldown`, `core`
/// WeaponData:34): a turret whose weapon does not lock it turns towards its target on every tick
/// of its backswing and cooldown, by at most its rate, while the target walks across its front.
#[test]
fn a_turret_follows_its_target_between_shots() -> TestResult {
    let world = shipped_world()?;
    let unit_type = find(&world, MoverType::Ground, |weapon| {
        weapon.has_turret
            && !weapon.locks_turret_while_firing
            && weapon.allowed_movement == AllowedMovement::Stopped
    })?;
    let rate = weapons(world.catalog.get(unit_type)?)
        .find(|weapon| weapon.has_turret)
        .ok_or("the turret")?
        .turret_turn_rate;
    let frames = between_shots_scene(unit_type, false, 80)?;
    let mut tracked = 0;
    for (tick, pair) in frames.windows(2).enumerate() {
        let ((before_phase, before, ..), (after_phase, after, _, bearing)) = (&pair[0], &pair[1]);
        let Some(bearing) = bearing else {
            continue;
        };
        if !(between_shots(before_phase) && between_shots(after_phase)) {
            continue;
        }
        assert_eq!(
            *after,
            before.rotate_towards(*bearing, rate.raw()),
            "tick {}: the turret turns towards its target at its rate",
            tick + 2
        );
        tracked += usize::from(after != before);
    }
    assert!(tracked > 0, "premise: the target moved between shots");
    Ok(())
}

/// B-1 of plan `2026-11-25-wind-up-and-hold-facing` (D-4, B-199; restates fire-rules P-12, R-7):
/// between shots a hull weapon of a unit on Hold Position, which no pursuit serves, turns the
/// hull towards its target: on every backswing and cooldown tick the hull turns by exactly the
/// stationary rate towards the target's bearing, or reaches it, while the target walks across
/// its front (5.0.14 `FaceTargetWhileInCooldown 1`, `core` WeaponData:34).
#[test]
fn between_shots_a_holding_hull_faces_its_target() -> TestResult {
    let world = shipped_world()?;
    let unit_type = world
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == MoverType::Ground
                && archetype.max_speed > SimScalar::ZERO
                && weapons(archetype).any(|weapon| {
                    !weapon.has_turret
                        && weapon.max_range >= SimScalar::from_i32(5)
                        && weapon.target_flags.intersects(archetype.attributes)
                })
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("a shipped ranged hull weapon")?
        .unit_type_id;
    let rate = world.catalog.get(unit_type)?.stationary_turn_rate;
    let frames = between_shots_scene(unit_type, true, 80)?;
    let mut between = 0;
    let mut turned = 0;
    for (tick, pair) in frames.windows(2).enumerate() {
        let ((before_phase, _, before, _), (after_phase, _, after, bearing)) = (&pair[0], &pair[1]);
        let Some(bearing) = bearing else {
            continue;
        };
        if !(between_shots(before_phase) && between_shots(after_phase)) {
            continue;
        }
        assert_eq!(
            *after,
            before.rotate_towards(*bearing, rate.raw()),
            "tick {}: the hull faces its target between shots",
            tick + 2
        );
        between += 1;
        turned += usize::from(after != before);
    }
    assert!(between > 0, "premise: the unit fired and cooled down");
    assert!(turned > 0, "premise: the target moved between shots");
    Ok(())
}
