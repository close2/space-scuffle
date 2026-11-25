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
//! A turret whose weapon tracks any enemy (`turret_tracks_any_enemy`; plan
//! `2026-11-26-turret-tracking-weapon`, B-198, B-218) aims at the nearest enemy in reach whenever
//! the real weapon leaves the turret free: under a Move, chasing a target out of reach, or between
//! shots without a target. The capability models either a hidden tracking weapon (5.0.14's
//! `90mmCannonsFake` on the unsieged Siege Tank) or a weapon whose attack-order options are
//! cleared while its movement gate blocks its fire as its unit walks (the Immortal's and the
//! Colossus', Amendment 2). It never changes an order, a target, a phase, a cooldown, a position
//! or a random draw: every such test compares its world with a twin whose catalog copy clears the
//! flag (T-1 to T-9, T-14, T-15 of that plan; T-A3 to T-A7 of Amendment 2).
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

// ---------------------------------------------------------------------------------------------
// The turret that tracks under any order (plan `2026-11-26-turret-tracking-weapon`, B-198):
// 5.0.14's unsieged Siege Tank carries the hidden `90mmCannonsFake` on its cannon's turret, so the
// turret aims at the nearest enemy in reach whenever the cannon leaves it free, and nothing else
// happens. The tests find the tracking unit by its weapon's `turret_tracks_any_enemy`, its
// enemies by capability, and compare each world with a twin whose catalog copy clears the flag.
// ---------------------------------------------------------------------------------------------

/// The shipped mobile ground archetype of lowest type id whose weapon's turret tracks any enemy
/// and whose weapon acquires and fires in every direction (half arcs of 180° or more), the
/// property the claims of T-1 to T-9, T-14 and T-15 rest on (T-4's same-tick swing, T-15's
/// scratch narrow arcs against a wide original): the Siege Tank analogue's tank-mode weapon, not
/// the narrow-arc carriers of Amendment 2, whatever the catalog's order.
fn tracker(world: &SimWorld) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    find(world, MoverType::Ground, |weapon| {
        weapon.turret_tracks_any_enemy
            && weapon.has_turret
            && weapon.acquire_arc >= SimAngle::DEG_180
            && weapon.firing_arc >= SimAngle::DEG_180
    })
}

/// The tracking weapon of `unit_type` (its ground weapon, unsieged).
fn tracking_weapon(
    world: &SimWorld,
    unit_type: UnitTypeId,
) -> Result<WeaponSpec, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .get(unit_type)?
        .weapon_against(MoverType::Ground, None)
        .filter(|weapon| weapon.turret_tracks_any_enemy)
        .ok_or("the tracker's ground weapon tracks")?
        .clone())
}

/// Edits every own weapon of `unit_type` in `world`'s copy of the catalog.
fn edit_weapons(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
    edit: impl Fn(&mut WeaponSpec),
) -> TestResult {
    let archetype = world.catalog.get_mut(unit_type)?;
    for weapon in [
        archetype.ground_weapon.as_mut(),
        archetype.air_weapon.as_mut(),
        archetype.weapon.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        edit(weapon);
    }
    Ok(())
}

/// The shipped mobile ground worker of lowest type id (a harvester that acquires only while
/// attack-moving, `AcquireLevel::Defensive`) that a weapon of `flags` may strike: idle, it never
/// attacks, and nothing in these scenes damages it.
fn worker(
    world: &SimWorld,
    flags: sim_types::AttributeFlags,
) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == MoverType::Ground
                && archetype.max_speed > SimScalar::ZERO
                && archetype.harvester.is_some()
                && archetype.acquire_level == sim_core::catalog::AcquireLevel::Defensive
                && flags.intersects(archetype.attributes)
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("a shipped worker the tracking weapon strikes")?
        .unit_type_id)
}

/// Spawns a unit of `unit_type` for player 0 at `at`, hull and turret facing `facing`.
fn spawn_facing(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
    at: SimVec2,
    facing: SimAngle,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let unit = world.spawn_unit(unit_type, PlayerId(0), at)?;
    let entity = world.entities.get_mut(&unit).ok_or("unit")?;
    entity.heading = facing;
    entity.turret_angle = facing;
    Ok(unit)
}

/// Whether `enemy` lies in the reach of `weapon` (range plus both collision radii; no minimum
/// range) from `unit`, on their current positions: acquisition's test.
fn in_reach(
    world: &SimWorld,
    weapon: &WeaponSpec,
    unit: EntityId,
    enemy: EntityId,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (Some(a), Some(b)) = (world.entities.get(&unit), world.entities.get(&enemy)) else {
        return Ok(false);
    };
    let radii = world.catalog.get(a.unit_type_id)?.collision_radius
        + world.catalog.get(b.unit_type_id)?.collision_radius;
    Ok(a.position.distance(b.position) <= weapon.max_range + radii)
}

/// The bearing from `from` to `to`, a binary angle.
fn bearing(
    world: &SimWorld,
    from: EntityId,
    to: EntityId,
) -> Result<SimAngle, Box<dyn std::error::Error>> {
    let a = world.entities.get(&from).ok_or("from")?;
    let b = world.entities.get(&to).ok_or("to")?;
    let delta = b.position - a.position;
    Ok(SimAngle::atan2(delta.y, delta.x))
}

/// Asserts that every entity of `a` equals its twin in `b` but for the turret's angle and the
/// tracked enemy, and that both generators are in the same state (stop rule 8 of the plan).
fn assert_twins(a: &SimWorld, b: &SimWorld, tick: u32) {
    assert_eq!(
        a.entities.keys().collect::<Vec<_>>(),
        b.entities.keys().collect::<Vec<_>>(),
        "tick {tick}: the same entities live"
    );
    for (id, entity) in &a.entities {
        let Some(twin) = b.entities.get(id) else {
            continue;
        };
        let mut same = entity.clone();
        same.turret_angle = twin.turret_angle;
        assert_eq!(
            &same, twin,
            "tick {tick}: entity {id:?} differs beyond its turret"
        );
    }
    assert!(a.rng == b.rng, "tick {tick}: the generators drew alike");
    assert_eq!(
        a.projectiles.len(),
        b.projectiles.len(),
        "tick {tick}: projectiles"
    );
}

/// The scene of T-1 to T-3 and T-14: a tracker walks east under a Move from (8, 30) to (56, 30),
/// hull and turret facing east, past two idle enemy workers standing 4 and 5 north of its path at
/// (30, 34) and (33, 35), within 5 of each other (a call for help would reach the second).
struct Pass {
    world: SimWorld,
    unit: EntityId,
    workers: [EntityId; 2],
    tick: u32,
}

impl Pass {
    /// The scene, with the tracker's flag cleared in this world's catalog copy unless `track`.
    fn new(track: bool) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with(tracker(&shipped_world()?)?, track)
    }

    /// The scene for a walker of `unit_type` (any shipped mobile unit whose ground weapon tracks),
    /// with its flag cleared in this world's catalog copy unless `track`.
    fn with(unit_type: UnitTypeId, track: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let mut world = shipped_world()?;
        let flags = world
            .catalog
            .get(unit_type)?
            .weapon_against(MoverType::Ground, None)
            .ok_or("the walker's ground weapon")?
            .target_flags;
        if !track {
            edit_weapons(&mut world, unit_type, |weapon| {
                weapon.turret_tracks_any_enemy = false;
            })?;
        }
        let worker_type = worker(&world, flags)?;
        let unit = spawn_facing(&mut world, unit_type, point(8, 30), SimAngle::ZERO)?;
        let workers = [
            world.spawn_unit(worker_type, PlayerId(1), point(30, 34))?,
            world.spawn_unit(worker_type, PlayerId(1), point(33, 35))?,
        ];
        step(
            &mut world,
            0,
            vec![order(
                0,
                unit,
                UnitOrder::Move {
                    target: point(56, 30),
                },
            )],
        );
        Ok(Self {
            world,
            unit,
            workers,
            tick: 1,
        })
    }

    /// Steps one tick without commands.
    fn step(&mut self) {
        step(&mut self.world, self.tick, Vec::new());
        self.tick += 1;
    }

    /// The nearest worker in reach of `weapon` now (ties to the lower id): the enemy acquisition
    /// chooses on the next tick (`AcquireTieBreak::Closest`, equal rank).
    fn nearest_in_reach(
        &self,
        weapon: &WeaponSpec,
    ) -> Result<Option<EntityId>, Box<dyn std::error::Error>> {
        let unit = self.world.entities.get(&self.unit).ok_or("unit")?;
        let mut best: Option<(SimScalar, EntityId)> = None;
        for &worker in &self.workers {
            if !in_reach(&self.world, weapon, self.unit, worker)? {
                continue;
            }
            let at = self.world.entities.get(&worker).ok_or("worker")?.position;
            let key = (unit.position.distance_sq(at), worker);
            if best.is_none_or(|held| key < held) {
                best = Some(key);
            }
        }
        Ok(best.map(|(_, id)| id))
    }
}

/// T-1 and T-14 (no stickiness) of plan `2026-11-26-turret-tracking-weapon` (D-3, D-4): while
/// its unit walks under a Move, a tracking turret turns on every tick by at most its rate towards
/// the nearest enemy in reach, switching to the other worker on the tick it becomes nearer, so
/// its bearing error never grows by more than the enemy's angular motion seen from the unit; with
/// no enemy in reach it idles as before (`ResetIfMoving`: towards the hull while moving).
#[test]
fn a_tracking_turret_follows_the_nearest_enemy_in_reach_under_a_move() -> TestResult {
    let mut scene = Pass::new(true)?;
    let unit_type = scene
        .world
        .entities
        .get(&scene.unit)
        .ok_or("unit")?
        .unit_type_id;
    let weapon = tracking_weapon(&scene.world, unit_type)?;
    let sim_types::TurretIdle::ResetIfMoving { rate: idle_rate } = weapon.turret_idle else {
        return Err("premise: the tracking turret resets to the hull while moving".into());
    };
    let (mut tracked, mut turned, mut reset_after) = (0, 0, 0);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..340 {
        let expected = scene.nearest_in_reach(&weapon)?;
        let before = scene
            .world
            .entities
            .get(&scene.unit)
            .ok_or("unit")?
            .turret_angle;
        scene.step();
        let tick = scene.tick - 1;
        let unit = scene.world.entities.get(&scene.unit).ok_or("unit")?;
        assert_eq!(
            scene.world.tracked_target(scene.unit),
            expected,
            "tick {tick}: the nearest enemy in reach is tracked"
        );
        assert_eq!(
            unit.order_queue.front(),
            Some(&UnitOrder::Move {
                target: point(56, 30)
            }),
            "tick {tick}: the move stays the order"
        );
        if let Some(enemy) = expected {
            let towards = bearing(&scene.world, scene.unit, enemy)?;
            assert_eq!(
                unit.turret_angle,
                before.rotate_towards(towards, weapon.turret_turn_rate.raw()),
                "tick {tick}: the turret turns towards the tracked enemy at its rate"
            );
            assert!(
                unit.turret_angle.diff(towards).unsigned_abs()
                    <= before.diff(towards).unsigned_abs(),
                "tick {tick}: the bearing error grows by no more than the enemy's angular motion"
            );
            tracked += 1;
            turned += usize::from(unit.turret_angle != before);
            seen.insert(enemy);
        } else {
            let moving = unit.velocity != SimVec2::ZERO;
            let idle = if moving {
                before.rotate_towards(unit.heading, idle_rate.raw())
            } else {
                before
            };
            assert_eq!(unit.turret_angle, idle, "tick {tick}: the turret idles");
            reset_after += usize::from(!seen.is_empty() && moving && idle != before);
        }
    }
    assert!(
        tracked > 20,
        "premise: the workers were in reach for a while ({tracked})"
    );
    assert!(turned > 10, "the turret turned while tracking ({turned})");
    assert_eq!(seen.len(), 2, "both workers were tracked in turn: {seen:?}");
    assert!(
        reset_after > 0,
        "after the pass the turret returned towards the hull"
    );
    Ok(())
}

/// T-2 and T-3 of plan `2026-11-26-turret-tracking-weapon` (D-5, stop rule 8): the pass of T-1
/// against its twin with the flag cleared in its catalog copy. On every tick every entity is equal
/// but for the turret's angle and the tracked enemy, and the generators are in the same state; no
/// projectile flies, the tracker never swings, the workers keep their life, place and (lack of)
/// orders: nothing is struck, nobody flees, nobody is alerted.
#[test]
fn tracking_changes_nothing_but_the_turret() -> TestResult {
    let mut on = Pass::new(true)?;
    let mut off = Pass::new(false)?;
    let workers = on.workers;
    let start: Vec<_> = workers
        .iter()
        .map(|id| {
            on.world
                .entities
                .get(id)
                .map(|w| (w.position, w.hp, w.shields))
        })
        .collect();
    let mut differ = 0;
    for _ in 0..340 {
        on.step();
        off.step();
        let tick = on.tick - 1;
        assert_twins(&on.world, &off.world, tick);
        let unit = on.world.entities.get(&on.unit).ok_or("unit")?;
        let twin = off.world.entities.get(&off.unit).ok_or("twin")?;
        differ += usize::from(unit.turret_angle != twin.turret_angle);
        assert_eq!(
            off.world.tracked_target(off.unit),
            None,
            "tick {tick}: the twin tracks nothing"
        );
        assert!(
            on.world.projectiles.is_empty(),
            "tick {tick}: nothing is fired"
        );
        assert!(
            !matches!(
                unit.weapon_phase,
                WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
            ),
            "tick {tick}: tracking never swings"
        );
        assert!(
            !unit.is_attack_revealed_to(PlayerId(1)),
            "tick {tick}: tracking reveals nothing"
        );
        for (index, id) in workers.iter().enumerate() {
            let worker = on.world.entities.get(id).ok_or("worker")?;
            assert_eq!(
                Some((worker.position, worker.hp, worker.shields)),
                start[index],
                "tick {tick}: worker {index} is untouched and stays"
            );
            assert!(
                matches!(worker.order_queue.front(), None | Some(UnitOrder::Idle)),
                "tick {tick}: worker {index} is neither alerted nor sent away"
            );
        }
    }
    assert!(
        differ > 10,
        "premise: the turrets differed ({differ} ticks)"
    );
    Ok(())
}

/// T-14 of plan `2026-11-26-turret-tracking-weapon` (D-3 step 4): a higher rank outranks a nearer
/// enemy. A melee threat holding 6 north of the path is tracked over an idle worker 3 north of it
/// on every tick both are in reach (threat first, `TargetPriorityKey::rank`).
#[test]
fn a_tracked_threat_outranks_a_nearer_worker() -> TestResult {
    let mut world = shipped_world()?;
    let unit_type = tracker(&world)?;
    let weapon = tracking_weapon(&world, unit_type)?;
    let unit_attributes = world.catalog.get(unit_type)?.attributes;
    let worker_type = worker(&world, weapon.target_flags)?;
    let threat_type = world
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == MoverType::Ground
                && archetype.max_speed > SimScalar::ZERO
                && archetype.acquire_level != sim_core::catalog::AcquireLevel::Defensive
                && weapon.target_flags.intersects(archetype.attributes)
                && archetype
                    .weapon_against(MoverType::Ground, None)
                    .is_some_and(|own| {
                        own.max_range < SimScalar::ONE
                            && own.target_flags.intersects(unit_attributes)
                    })
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("a shipped melee ground unit that threatens the tracker")?
        .unit_type_id;
    let unit = spawn_facing(&mut world, unit_type, point(8, 30), SimAngle::ZERO)?;
    let near = world.spawn_unit(worker_type, PlayerId(1), point(26, 33))?;
    let threat = world.spawn_unit(threat_type, PlayerId(1), point(27, 36))?;
    step(
        &mut world,
        0,
        vec![
            order(
                0,
                unit,
                UnitOrder::Move {
                    target: point(56, 30),
                },
            ),
            order(1, threat, UnitOrder::HoldPosition),
        ],
    );
    let mut both = 0;
    for tick in 1..300 {
        let reach = (
            in_reach(&world, &weapon, unit, near)?,
            in_reach(&world, &weapon, unit, threat)?,
        );
        step(&mut world, tick, Vec::new());
        let tracked = world.tracked_target(unit);
        if reach == (true, true) {
            assert_eq!(
                tracked,
                Some(threat),
                "tick {tick}: the threat outranks the worker"
            );
            both += 1;
        }
    }
    assert!(
        both > 5,
        "premise: both enemies were in reach together ({both})"
    );
    Ok(())
}

/// T-4 of plan `2026-11-26-turret-tracking-weapon` (D-4 "free" includes a target out of band):
/// under a player's attack on a worker out of reach, the turret turns to another worker in reach
/// while the order, `current_target` and the chase stay on the order's target, equal to the
/// flag-off twin on every tick but the turret; once the target is in the band the real weapon
/// takes the turret, and the first swing starts on the same tick as the twin's (the tracker's
/// weapon fires in every direction).
#[test]
fn a_chasing_tracker_aims_at_another_enemy_until_its_target_is_in_reach() -> TestResult {
    let scene = |track: bool| -> Result<
        (SimWorld, EntityId, EntityId, EntityId),
        Box<dyn std::error::Error>,
    > {
        let mut world = shipped_world()?;
        let unit_type = tracker(&world)?;
        let flags = tracking_weapon(&world, unit_type)?.target_flags;
        if !track {
            edit_weapons(&mut world, unit_type, |w| w.turret_tracks_any_enemy = false)?;
        }
        let worker_type = worker(&world, flags)?;
        let unit = spawn_facing(&mut world, unit_type, point(10, 30), SimAngle::ZERO)?;
        let target = world.spawn_unit(worker_type, PlayerId(1), point(40, 30))?;
        let other = world.spawn_unit(worker_type, PlayerId(1), point(13, 35))?;
        step(
            &mut world,
            0,
            vec![order(0, unit, UnitOrder::attack(target))],
        );
        Ok((world, unit, target, other))
    };
    let (mut on, unit, target, other) = scene(true)?;
    let (mut off, ..) = scene(false)?;
    let weapon = tracking_weapon(&on, on.entities.get(&unit).ok_or("unit")?.unit_type_id)?;
    let (mut tracked, mut first_swing) = (0, None);
    for tick in 1..200 {
        let other_in_reach = in_reach(&on, &weapon, unit, other)?;
        let target_in_reach = in_reach(&on, &weapon, unit, target)?;
        step(&mut on, tick, Vec::new());
        step(&mut off, tick, Vec::new());
        assert_twins(&on, &off, tick);
        let entity = on.entities.get(&unit).ok_or("unit")?;
        if !target_in_reach && on.entities.contains_key(&target) {
            assert_eq!(
                entity.order_queue.front(),
                Some(&UnitOrder::attack(target)),
                "tick {tick}: the attack stays the order"
            );
            assert_eq!(
                entity.current_target,
                Some(target),
                "tick {tick}: the target stays"
            );
            let expected = other_in_reach.then_some(other);
            assert_eq!(
                on.tracked_target(unit),
                expected,
                "tick {tick}: the other worker is tracked"
            );
            tracked += usize::from(expected.is_some());
        }
        if first_swing.is_none() && matches!(entity.weapon_phase, WeaponPhase::Windup { .. }) {
            first_swing = Some(tick);
            assert!(
                matches!(
                    off.entities.get(&unit).ok_or("twin")?.weapon_phase,
                    WeaponPhase::Windup { .. }
                ),
                "tick {tick}: the twin's first swing starts on the same tick"
            );
        }
    }
    assert!(
        tracked > 5,
        "premise: the other worker was tracked during the chase ({tracked})"
    );
    assert!(
        first_swing.is_some(),
        "premise: the tracker reached its target and swung"
    );
    Ok(())
}

/// T-5 of plan `2026-11-26-turret-tracking-weapon` (D-4 rule 1, Q0109 part 3): under a player's
/// attack, and on Hold Position, with a workable target and a nearer enemy in reach, the real
/// weapon owns the turret through wind-up, backswing and cooldown: nothing is tracked, and the
/// world equals its flag-off twin on every tick, turrets included.
#[test]
fn the_real_weapon_owns_the_turret_while_it_has_a_workable_target() -> TestResult {
    for hold in [false, true] {
        let scene = |track: bool| -> Result<(SimWorld, EntityId), Box<dyn std::error::Error>> {
            let mut world = shipped_world()?;
            let unit_type = tracker(&world)?;
            let flags = tracking_weapon(&world, unit_type)?.target_flags;
            if !track {
                edit_weapons(&mut world, unit_type, |w| w.turret_tracks_any_enemy = false)?;
            }
            let worker_type = worker(&world, flags)?;
            let unit = spawn_facing(&mut world, unit_type, point(20, 30), SimAngle::ZERO)?;
            let far = world.spawn_unit(worker_type, PlayerId(1), point(26, 30))?;
            let near = world.spawn_unit(worker_type, PlayerId(1), point(20, 33))?;
            // Durable, so that no target dies and leaves the tracker without one.
            for id in [far, near] {
                let worker = world.entities.get_mut(&id).ok_or("worker")?;
                worker.max_hp = SimScalar::from_i32(100_000);
                worker.hp = worker.max_hp;
            }
            let command = if hold {
                UnitOrder::HoldPosition
            } else {
                UnitOrder::attack(far)
            };
            step(&mut world, 0, vec![order(0, unit, command)]);
            Ok((world, unit))
        };
        let (mut on, unit) = scene(true)?;
        let (mut off, _) = scene(false)?;
        let mut phases = Vec::new();
        for tick in 1..120 {
            step(&mut on, tick, Vec::new());
            step(&mut off, tick, Vec::new());
            let entity = on.entities.get(&unit).ok_or("unit")?;
            if !hold {
                assert_eq!(
                    on.tracked_target(unit),
                    None,
                    "tick {tick}: the order's target owns the turret"
                );
                assert_eq!(
                    entity.turret_angle,
                    off.entities.get(&unit).ok_or("twin")?.turret_angle,
                    "tick {tick}: the turret is the real weapon's"
                );
            }
            assert_twins(&on, &off, tick);
            let phase = std::mem::discriminant(&entity.weapon_phase);
            if !phases.contains(&phase) {
                phases.push(phase);
            }
        }
        assert!(
            phases.len() >= 3,
            "premise: the weapon wound up, followed through and cooled down"
        );
    }
    Ok(())
}

/// T-6 of plan `2026-11-26-turret-tracking-weapon` (D-5, between shots): the tracker kills its
/// attack's target with one shot, then walks away under a queued Move while its cooldown runs;
/// another worker in reach is tracked and the turret turns to it, while the cooldown runs out on
/// the same tick as in the flag-off twin (every entity but the turret equal on every tick).
#[test]
fn between_shots_without_a_target_the_turret_tracks() -> TestResult {
    let scene =
        |track: bool| -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
            let mut world = shipped_world()?;
            let unit_type = tracker(&world)?;
            let flags = tracking_weapon(&world, unit_type)?.target_flags;
            if !track {
                edit_weapons(&mut world, unit_type, |w| w.turret_tracks_any_enemy = false)?;
            }
            let worker_type = worker(&world, flags)?;
            let unit = spawn_facing(&mut world, unit_type, point(20, 30), SimAngle::ZERO)?;
            let victim = world.spawn_unit(worker_type, PlayerId(1), point(25, 30))?;
            let other = world.spawn_unit(worker_type, PlayerId(1), point(17, 35))?;
            world.entities.get_mut(&victim).ok_or("victim")?.hp = SimScalar::ONE;
            let queued = PlayerCommand {
                queue: true,
                ..order(
                    0,
                    unit,
                    UnitOrder::Move {
                        target: point(20, 20),
                    },
                )
            };
            step(
                &mut world,
                0,
                vec![order(0, unit, UnitOrder::attack(victim)), queued],
            );
            Ok((world, unit, other))
        };
    let (mut on, unit, other) = scene(true)?;
    let (mut off, ..) = scene(false)?;
    let (mut cooling_tracked, mut turned) = (0, 0);
    for tick in 1..80 {
        let before = on.entities.get(&unit).ok_or("unit")?.turret_angle;
        step(&mut on, tick, Vec::new());
        step(&mut off, tick, Vec::new());
        assert_twins(&on, &off, tick);
        let entity = on.entities.get(&unit).ok_or("unit")?;
        if on.tracked_target(unit) == Some(other) && entity.weapon_cooldown_remaining > 0 {
            cooling_tracked += 1;
            turned += usize::from(entity.turret_angle != before);
        }
    }
    assert!(
        cooling_tracked > 0,
        "premise: the other worker was tracked during the cooldown"
    );
    assert!(
        turned > 0,
        "the turret turned to the other worker between shots"
    );
    Ok(())
}

/// T-7 of plan `2026-11-26-turret-tracking-weapon` (D-4 rule 4): a turret whose weapon also locks
/// it while firing does not turn for tracking during its swing. A scratch copy of the tracker's
/// weapon with the lock kills a one-life worker with its first shot; through the backswing that
/// follows the turret holds although another worker is tracked, and it turns to that worker once
/// the backswing has ended. The tracker acquires only while attack-moving in this copy
/// (`AcquireLevel::Defensive`), so it does not engage the other worker on its own.
#[test]
fn a_locked_turret_does_not_track_during_its_swing() -> TestResult {
    let mut world = shipped_world()?;
    let unit_type = tracker(&world)?;
    let flags = tracking_weapon(&world, unit_type)?.target_flags;
    edit_weapons(&mut world, unit_type, |w| {
        w.locks_turret_while_firing = true;
    })?;
    world.catalog.get_mut(unit_type)?.acquire_level = sim_core::catalog::AcquireLevel::Defensive;
    let worker_type = worker(&world, flags)?;
    let unit = spawn_facing(&mut world, unit_type, point(20, 30), SimAngle::ZERO)?;
    let victim = world.spawn_unit(worker_type, PlayerId(1), point(25, 30))?;
    let other = world.spawn_unit(worker_type, PlayerId(1), point(20, 35))?;
    world.entities.get_mut(&victim).ok_or("victim")?.hp = SimScalar::ONE;
    step(
        &mut world,
        0,
        vec![order(0, unit, UnitOrder::attack(victim))],
    );
    let (mut held, mut turned_after) = (0, 0);
    let mut released = false;
    for tick in 1..60 {
        let before = world.entities.get(&unit).ok_or("unit")?.clone();
        step(&mut world, tick, Vec::new());
        let entity = world.entities.get(&unit).ok_or("unit")?;
        released |= !world.entities.contains_key(&victim);
        if !released {
            continue;
        }
        let swinging = matches!(before.weapon_phase, WeaponPhase::Backswing { .. })
            && matches!(entity.weapon_phase, WeaponPhase::Backswing { .. });
        if swinging && world.tracked_target(unit) == Some(other) {
            assert_eq!(
                entity.turret_angle, before.turret_angle,
                "tick {tick}: the lock holds"
            );
            held += 1;
        } else if world.tracked_target(unit) == Some(other)
            && entity.turret_angle != before.turret_angle
        {
            turned_after += 1;
        }
    }
    assert!(
        held > 0,
        "premise: the other worker was tracked during the backswing"
    );
    assert!(
        turned_after > 0,
        "after the backswing the turret turns to the tracked worker"
    );
    Ok(())
}

/// The ability index of `unit_type` that switches into its first mode whose ground weapon is
/// replaced by one without turret tracking (the sieged stance), and that mode's channel length.
fn tracking_off_mode(
    world: &SimWorld,
    unit_type: UnitTypeId,
) -> Result<u8, Box<dyn std::error::Error>> {
    let archetype = world.catalog.get(unit_type)?;
    let mode = archetype
        .modes
        .iter()
        .find(|mode| {
            mode.ground_weapon_override
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|weapon| !weapon.turret_tracks_any_enemy)
        })
        .ok_or("the tracker has a mode whose weapon does not track")?;
    let index = archetype
        .abilities
        .iter()
        .position(|ability| {
            matches!(&ability.effect,
                sim_types::AbilityEffectSpec::SwitchMode { mode_name } if *mode_name == mode.mode_name)
        })
        .ok_or("the mode has a switch ability")?;
    Ok(u8::try_from(index)?)
}

/// T-8 of plan `2026-11-26-turret-tracking-weapon` (D-5, S6): while the tracker channels its
/// switch into a stance whose weapon does not track, the turret does not turn and nothing is
/// tracked although an enemy stands in reach; in that stance nothing is ever tracked.
#[test]
fn a_mode_channel_and_a_stance_without_tracking_track_nothing() -> TestResult {
    let mut world = shipped_world()?;
    let unit_type = tracker(&world)?;
    let flags = tracking_weapon(&world, unit_type)?.target_flags;
    let ability_index = tracking_off_mode(&world, unit_type)?;
    let worker_type = worker(&world, flags)?;
    let unit = spawn_facing(&mut world, unit_type, point(20, 30), SimAngle::ZERO)?;
    let enemy = world.spawn_unit(worker_type, PlayerId(1), point(20, 35))?;
    step(
        &mut world,
        0,
        vec![
            order(
                0,
                unit,
                UnitOrder::CastAbility {
                    ability_index,
                    target_pos: None,
                    target_entity: None,
                },
            ),
            order(1, enemy, UnitOrder::HoldPosition),
        ],
    );
    let (mut channel, mut sieged) = (0, 0);
    for tick in 1..200 {
        let before = world.entities.get(&unit).ok_or("unit")?.turret_angle;
        step(&mut world, tick, Vec::new());
        let entity = world.entities.get(&unit).ok_or("unit")?;
        assert_eq!(
            world.tracked_target(unit),
            None,
            "tick {tick}: nothing is tracked"
        );
        if entity.channeling_remaining_ticks > 0 {
            assert_eq!(
                entity.turret_angle, before,
                "tick {tick}: no turret turns in a channel"
            );
            channel += 1;
        } else if entity.active_siege_weapon().is_some() {
            sieged += 1;
        }
    }
    assert!(
        channel > 10,
        "premise: the switch channelled ({channel} ticks)"
    );
    assert!(sieged > 10, "premise: the stance was held ({sieged} ticks)");
    Ok(())
}

/// T-9 of plan `2026-11-26-turret-tracking-weapon` (D-3 step 3, D-5): only enemies the tracker's
/// player sees and its weapon may strike are tracked. On a map whose north half is high ground,
/// a tracker walking east below it never tracks a worker on the high ground, a cloaked worker no
/// detector reveals, a worker carried in a transport, or a dead one, all in reach; a visible
/// worker that is tracked is released on the tick after it is moved up onto the high ground.
#[test]
fn only_seen_strikeable_enemies_are_tracked() -> TestResult {
    let (width, height) = (64_u32, 64_u32);
    let mut cells = vec![MapCell::default_ground(CliffLevel(1)); (width * height) as usize];
    for y in 36..height {
        for x in 0..width {
            cells[(y * width + x) as usize] = MapCell::default_ground(CliffLevel(2));
        }
    }
    let map = GameMap::from_cells(width, height, cells, Vec::new(), Vec::new())?;
    let mut world = sim_fixtures::world(map, 0x9876_5432);
    let unit_type = tracker(&world)?;
    let weapon = tracking_weapon(&world, unit_type)?;
    let worker_type = worker(&world, weapon.target_flags)?;
    let carrier_type = world
        .catalog
        .archetypes()
        .filter(|archetype| archetype.capabilities.transport_capacity > 0)
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("a shipped transport")?
        .unit_type_id;
    let unit = spawn_facing(&mut world, unit_type, point(8, 32), SimAngle::ZERO)?;
    let high = world.spawn_unit(worker_type, PlayerId(1), point(14, 38))?;
    let cloaked = world.spawn_unit(worker_type, PlayerId(1), point(16, 30))?;
    world
        .entities
        .get_mut(&cloaked)
        .ok_or("cloaked")?
        .is_cloaked = true;
    let carrier = world.spawn_unit(carrier_type, PlayerId(1), point(18, 33))?;
    let carried = world.spawn_unit(worker_type, PlayerId(1), point(18, 34))?;
    world.try_load_unit_into_transport(carried, carrier)?;
    let dead = world.spawn_unit(worker_type, PlayerId(1), point(12, 30))?;
    world.entities.get_mut(&dead).ok_or("dead")?.hp = SimScalar::ZERO;
    let seen = world.spawn_unit(worker_type, PlayerId(1), point(30, 34))?;
    step(
        &mut world,
        0,
        vec![
            order(
                0,
                unit,
                UnitOrder::Move {
                    target: point(56, 32),
                },
            ),
            order(1, carrier, UnitOrder::HoldPosition),
        ],
    );
    let never = [high, cloaked, carried, dead];
    let (mut tracked_seen, mut lifted) = (0, None);
    for tick in 1..300 {
        step(&mut world, tick, Vec::new());
        let tracked = world.tracked_target(unit);
        assert!(
            tracked.is_none_or(|id| !never.contains(&id)),
            "tick {tick}: an unseen or unstrikeable enemy is never tracked ({tracked:?})"
        );
        if let Some(at) = lifted {
            if tick == at + 1 {
                assert_ne!(
                    tracked,
                    Some(seen),
                    "tick {tick}: the lifted worker is released"
                );
            }
        } else if tracked == Some(seen) {
            tracked_seen += 1;
            if tracked_seen == 3 {
                world.entities.get_mut(&seen).ok_or("seen")?.position = point(31, 38);
                lifted = Some(tick);
            }
        }
    }
    for id in never {
        if id == dead {
            continue;
        }
        assert!(
            world.entities.contains_key(&id),
            "premise: the hidden enemies stayed in the world"
        );
    }
    assert!(lifted.is_some(), "premise: the visible worker was tracked");
    Ok(())
}

/// The half arc of T-15's scratch weapon: an eighth of a right angle (11.25°).
fn narrow_arc() -> SimAngle {
    SimAngle::from_raw(SimAngle::DEG_90.raw() / 8)
}

/// T-15's scene: a tracker whose weapon copy acquires and fires only within [`narrow_arc`], its
/// turret tracking any enemy if `track`, walks east from (10, 30) under a Move past a durable
/// idle worker at (20, 35). Returns the world, the tracker and the worker.
fn narrow_scene(track: bool) -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
    let narrow = narrow_arc();
    let mut world = shipped_world()?;
    let unit_type = tracker(&world)?;
    let flags = tracking_weapon(&world, unit_type)?.target_flags;
    edit_weapons(&mut world, unit_type, |w| {
        w.acquire_arc = narrow;
        w.firing_arc = narrow;
        w.turret_tracks_any_enemy = track;
    })?;
    let worker_type = worker(&world, flags)?;
    let unit = spawn_facing(&mut world, unit_type, point(10, 30), SimAngle::ZERO)?;
    let enemy = world.spawn_unit(worker_type, PlayerId(1), point(20, 35))?;
    let durable = world.entities.get_mut(&enemy).ok_or("enemy")?;
    durable.max_hp = SimScalar::from_i32(100_000);
    durable.hp = durable.max_hp;
    let walk = UnitOrder::Move {
        target: point(56, 30),
    };
    step(&mut world, 0, vec![order(0, unit, walk)]);
    Ok((world, unit, enemy))
}

/// The first tick from `from` on, within 60, after which `unit` is winding up.
fn first_swing_from(
    world: &mut SimWorld,
    unit: EntityId,
    from: u32,
) -> Result<Option<u32>, Box<dyn std::error::Error>> {
    for tick in from..from + 60 {
        step(world, tick, Vec::new());
        if matches!(
            world.entities.get(&unit).ok_or("unit")?.weapon_phase,
            WeaponPhase::Windup { .. }
        ) {
            return Ok(Some(tick));
        }
    }
    Ok(None)
}

/// T-15 of plan `2026-11-26-turret-tracking-weapon` (§6.3 class P): the only channel from tracking
/// to the real weapon is the turret's angle. A scratch copy of the tracker's weapon with a narrow
/// acquisition and firing arc tracks a worker while its unit walks, then is ordered to attack it:
/// its first swing starts no later than the flag-off twin's. A third world, tracking, whose
/// turret is set to the twin's on the tick of the order, equals the twin on every later tick.
#[test]
fn tracking_reaches_the_real_weapon_only_through_the_turret_angle() -> TestResult {
    let narrow = narrow_arc();
    let (mut on, unit, enemy) = narrow_scene(true)?;
    let (mut off, ..) = narrow_scene(false)?;
    let (mut equalized, ..) = narrow_scene(true)?;
    let mut order_tick = None;
    for tick in 1..120 {
        step(&mut on, tick, Vec::new());
        step(&mut off, tick, Vec::new());
        step(&mut equalized, tick, Vec::new());
        let entity = on.entities.get(&unit).ok_or("unit")?;
        let twin = off.entities.get(&unit).ok_or("twin")?;
        if on.tracked_target(unit) == Some(enemy)
            && entity.turret_angle.diff(twin.turret_angle).unsigned_abs() > narrow.raw()
        {
            order_tick = Some(tick + 1);
            break;
        }
    }
    let order_tick =
        order_tick.ok_or("premise: the tracked turret turned well away from the hull")?;
    let attack = || vec![order(0, unit, UnitOrder::attack(enemy))];
    let twin_turret = off.entities.get(&unit).ok_or("twin")?.turret_angle;
    equalized
        .entities
        .get_mut(&unit)
        .ok_or("unit")?
        .turret_angle = twin_turret;
    let winding = |world: &SimWorld| {
        world
            .entities
            .get(&unit)
            .is_some_and(|e| matches!(e.weapon_phase, WeaponPhase::Windup { .. }))
    };
    step(&mut on, order_tick, attack());
    step(&mut off, order_tick, attack());
    step(&mut equalized, order_tick, attack());
    assert_twins(&equalized, &off, order_tick);
    let on_swing = if winding(&on) {
        order_tick
    } else {
        first_swing_from(&mut on, unit, order_tick + 1)?.ok_or("the tracker swings")?
    };
    let mut off_copy_ticks = Vec::new();
    if winding(&off) {
        off_copy_ticks.push(order_tick);
    }
    for tick in order_tick + 1..order_tick + 61 {
        step(&mut off, tick, Vec::new());
        step(&mut equalized, tick, Vec::new());
        assert_twins(&equalized, &off, tick);
        assert_eq!(
            equalized.entities.get(&unit).ok_or("unit")?.turret_angle,
            off.entities.get(&unit).ok_or("twin")?.turret_angle,
            "tick {tick}: with equal turrets the worlds stay equal"
        );
        if matches!(
            off.entities.get(&unit).ok_or("twin")?.weapon_phase,
            WeaponPhase::Windup { .. }
        ) {
            off_copy_ticks.push(tick);
        }
    }
    let off_swing = *off_copy_ticks.first().ok_or("the twin swings")?;
    assert!(
        on_swing <= off_swing,
        "the pre-aimed swing starts no later ({on_swing} vs {off_swing})"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The turret that aims while its unit walks (plan `2026-11-26-turret-tracking-weapon` Amendment 2,
// B-218, Q0116 part 2 (b)): 5.0.14's Immortal and Colossus weapons clear both attack-order options
// (`voidmulti` WeaponData:205–207, :256–257) but may not fire while their unit walks, so, like the
// tank's hidden weapon, they turn their turret to the nearest enemy in reach under a Move and fire
// nothing until the unit stops. The tests find these carriers by property (a turreted weapon that
// may fire outside attack orders but not while moving), never by name, and compare each world with
// a twin whose catalog copy clears the flag (T-A3 to T-A7 of the amendment).
// ---------------------------------------------------------------------------------------------

/// Whether `weapon` is of the amendment's class (A2.1 S20): turreted, it may fire outside attack
/// orders (`fires_outside_attack_order`), and its unit must slow or stand to fire
/// (`allowed_movement` other than `Moving`).
fn aims_only_while_walking(weapon: &WeaponSpec) -> bool {
    weapon.has_turret
        && weapon.fires_outside_attack_order
        && weapon.allowed_movement != AllowedMovement::Moving
}

/// Every shipped mobile ground-plane archetype (not `Air`) whose ground weapon is of the
/// amendment's class, by type id: the Immortal and Colossus analogues in the shipped data.
fn walking_carriers(world: &SimWorld) -> Result<Vec<UnitTypeId>, Box<dyn std::error::Error>> {
    let carriers: Vec<_> = world
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type != MoverType::Air
                && archetype.max_speed > SimScalar::ZERO
                && archetype
                    .weapon_against(MoverType::Ground, None)
                    .is_some_and(aims_only_while_walking)
        })
        .map(|archetype| archetype.unit_type_id)
        .collect();
    if carriers.is_empty() {
        return Err(
            "premise: a shipped carrier that may fire outside attack orders but not while \
                    moving"
                .into(),
        );
    }
    Ok(carriers)
}

/// The ground weapon of carrier `unit_type`, the amendment's class (whether or not its flag is
/// set, so that a world without the flag reaches the behaviour's assertions).
fn carrier_weapon(
    world: &SimWorld,
    unit_type: UnitTypeId,
) -> Result<WeaponSpec, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .get(unit_type)?
        .weapon_against(MoverType::Ground, None)
        .filter(|weapon| aims_only_while_walking(weapon))
        .ok_or("the carrier's ground weapon")?
        .clone())
}

/// The rate at which `weapon`'s idle turret returns to the hull on a tick its unit moved
/// (`ResetIfMoving` or `FollowMovement`, which `idle_turret` treats alike).
fn idle_rate(weapon: &WeaponSpec) -> Result<SimAngle, Box<dyn std::error::Error>> {
    match weapon.turret_idle {
        sim_types::TurretIdle::ResetIfMoving { rate }
        | sim_types::TurretIdle::FollowMovement { rate } => Ok(rate),
        _ => Err("premise: the turret returns to the hull while its unit moves".into()),
    }
}

/// T-A3 of Amendment 2 (T-1's conditions, A2.2): each carrier walking under a Move past two idle
/// workers turns its turret on every tick by at most its rate towards the nearest worker in
/// reach, both in turn, while the Move stays its order; with no enemy in reach its idle runs as
/// before (`FollowMovement`: towards the hull while it moves).
#[test]
fn a_carrier_that_stands_to_fire_tracks_while_it_walks() -> TestResult {
    for unit_type in walking_carriers(&shipped_world()?)? {
        let mut scene = Pass::with(unit_type, true)?;
        let weapon = carrier_weapon(&scene.world, unit_type)?;
        let rate = idle_rate(&weapon)?;
        let walk = UnitOrder::Move {
            target: point(56, 30),
        };
        let (mut tracked, mut turned, mut idled_after) = (0, 0, 0);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..600 {
            let expected = scene.nearest_in_reach(&weapon)?;
            let before = scene
                .world
                .entities
                .get(&scene.unit)
                .ok_or("unit")?
                .turret_angle;
            scene.step();
            let tick = scene.tick - 1;
            let unit = scene.world.entities.get(&scene.unit).ok_or("unit")?;
            if unit.order_queue.front() != Some(&walk) {
                break;
            }
            assert_eq!(
                scene.world.tracked_target(scene.unit),
                expected,
                "{unit_type:?} tick {tick}: the nearest enemy in reach is tracked"
            );
            if let Some(enemy) = expected {
                let towards = bearing(&scene.world, scene.unit, enemy)?;
                assert_eq!(
                    unit.turret_angle,
                    before.rotate_towards(towards, weapon.turret_turn_rate.raw()),
                    "{unit_type:?} tick {tick}: the turret turns towards the enemy at its rate"
                );
                tracked += 1;
                turned += usize::from(unit.turret_angle != before);
                seen.insert(enemy);
            } else {
                let moving = unit.velocity != SimVec2::ZERO;
                let idle = if moving {
                    before.rotate_towards(unit.heading, rate.raw())
                } else {
                    before
                };
                assert_eq!(
                    unit.turret_angle, idle,
                    "{unit_type:?} tick {tick}: the turret idles"
                );
                idled_after += usize::from(!seen.is_empty() && moving && idle != before);
            }
        }
        let unit = scene.world.entities.get(&scene.unit).ok_or("unit")?;
        assert_ne!(
            unit.order_queue.front(),
            Some(&walk),
            "{unit_type:?}: premise: the Move ended"
        );
        assert!(
            tracked > 20,
            "{unit_type:?}: premise: the workers were in reach ({tracked})"
        );
        assert!(
            turned > 10,
            "{unit_type:?}: the turret turned while tracking ({turned})"
        );
        assert_eq!(
            seen.len(),
            2,
            "{unit_type:?}: both workers in turn: {seen:?}"
        );
        assert!(
            idled_after > 0,
            "{unit_type:?}: after the pass the turret returned towards the hull"
        );
    }
    Ok(())
}

/// T-A4 of Amendment 2 (T-2's and T-3's comparisons, stop rule A-4): over each carrier's Move past
/// the workers, every entity equals its flag-off twin's but for the turret and the generators are
/// alike; nothing is fired, the carrier never swings or reveals itself, and the workers keep
/// their life, place and (lack of) orders.
#[test]
fn a_walking_carrier_changes_nothing_but_its_turret() -> TestResult {
    for unit_type in walking_carriers(&shipped_world()?)? {
        let mut on = Pass::with(unit_type, true)?;
        let mut off = Pass::with(unit_type, false)?;
        let walk = UnitOrder::Move {
            target: point(56, 30),
        };
        let start: Vec<_> = on
            .workers
            .iter()
            .map(|id| {
                on.world
                    .entities
                    .get(id)
                    .map(|w| (w.position, w.hp, w.shields))
            })
            .collect();
        let (mut differ, mut walked) = (0, 0);
        loop {
            let walking = on
                .world
                .entities
                .get(&on.unit)
                .is_some_and(|unit| unit.order_queue.front() == Some(&walk));
            if !walking || on.tick > 600 {
                break;
            }
            on.step();
            off.step();
            walked += 1;
            let tick = on.tick - 1;
            assert_twins(&on.world, &off.world, tick);
            let unit = on.world.entities.get(&on.unit).ok_or("unit")?;
            let twin = off.world.entities.get(&off.unit).ok_or("twin")?;
            differ += usize::from(unit.turret_angle != twin.turret_angle);
            assert_eq!(
                off.world.tracked_target(off.unit),
                None,
                "tick {tick}: twin"
            );
            assert!(
                on.world.projectiles.is_empty(),
                "tick {tick}: nothing fired"
            );
            assert!(
                !matches!(
                    unit.weapon_phase,
                    WeaponPhase::Windup { .. } | WeaponPhase::Backswing { .. }
                ),
                "{unit_type:?} tick {tick}: tracking never swings"
            );
            assert!(
                !unit.is_attack_revealed_to(PlayerId(1)),
                "{unit_type:?} tick {tick}: tracking reveals nothing"
            );
            for (index, id) in on.workers.iter().enumerate() {
                let worker = on.world.entities.get(id).ok_or("worker")?;
                assert_eq!(
                    Some((worker.position, worker.hp, worker.shields)),
                    start[index],
                    "{unit_type:?} tick {tick}: worker {index} is untouched and stays"
                );
                assert!(
                    matches!(worker.order_queue.front(), None | Some(UnitOrder::Idle)),
                    "{unit_type:?} tick {tick}: worker {index} is neither alerted nor sent away"
                );
            }
        }
        assert!(
            walked < 600,
            "{unit_type:?}: premise: the Move ended ({walked} ticks)"
        );
        assert!(
            differ > 10,
            "{unit_type:?}: premise: the turrets differed ({differ} ticks)"
        );
    }
    Ok(())
}

/// T-A5 of Amendment 2 (the chase, fire-rules P-5): each carrier under a player's attack on a
/// worker out of reach, with another worker in reach, tracks that other worker and never swings
/// at it (its only swing, if any, starts at the order's target on the tick it comes into reach);
/// its order, `current_target`, `weapon_target` (`None`: its weapon may not fire while
/// the unit chases) and pursuit equal the flag-off twin's on every tick until the target comes
/// into reach.
#[test]
fn a_chasing_carrier_aims_at_another_enemy_and_never_swings_at_it() -> TestResult {
    for unit_type in walking_carriers(&shipped_world()?)? {
        let scene = |track: bool| -> Result<
            (SimWorld, EntityId, EntityId, EntityId),
            Box<dyn std::error::Error>,
        > {
            let mut world = shipped_world()?;
            let flags = carrier_weapon(&world, unit_type)?.target_flags;
            if !track {
                edit_weapons(&mut world, unit_type, |w| w.turret_tracks_any_enemy = false)?;
            }
            let worker_type = worker(&world, flags)?;
            let unit = spawn_facing(&mut world, unit_type, point(10, 30), SimAngle::ZERO)?;
            let target = world.spawn_unit(worker_type, PlayerId(1), point(40, 30))?;
            let other = world.spawn_unit(worker_type, PlayerId(1), point(13, 35))?;
            step(
                &mut world,
                0,
                vec![order(0, unit, UnitOrder::attack(target))],
            );
            Ok((world, unit, target, other))
        };
        let (mut on, unit, target, other) = scene(true)?;
        let (mut off, ..) = scene(false)?;
        let weapon = carrier_weapon(&on, unit_type)?;
        let mut tracked = 0;
        for tick in 1..400 {
            let other_in_reach = in_reach(&on, &weapon, unit, other)?;
            if in_reach(&on, &weapon, unit, target)? {
                break;
            }
            step(&mut on, tick, Vec::new());
            step(&mut off, tick, Vec::new());
            assert_twins(&on, &off, tick);
            let entity = on.entities.get(&unit).ok_or("unit")?;
            assert_eq!(
                entity.order_queue.front(),
                Some(&UnitOrder::attack(target)),
                "{unit_type:?} tick {tick}: the attack stays the order"
            );
            assert_eq!(entity.current_target, Some(target), "tick {tick}: target");
            assert_eq!(
                entity.weapon_target, None,
                "{unit_type:?} tick {tick}: no own target while chasing"
            );
            // A swing may start on the tick the chase brings the order's target into reach; it is
            // never one at the tracked enemy.
            if let WeaponPhase::Windup { target_id, .. }
            | WeaponPhase::Backswing { target_id, .. } = entity.weapon_phase
            {
                assert_eq!(
                    target_id, target,
                    "{unit_type:?} tick {tick}: no swing at the tracked enemy"
                );
            }
            let expected = other_in_reach.then_some(other);
            assert_eq!(
                on.tracked_target(unit),
                expected,
                "{unit_type:?} tick {tick}: the other worker is tracked"
            );
            tracked += usize::from(expected.is_some());
        }
        assert!(
            tracked > 5,
            "{unit_type:?}: premise: the other worker was tracked ({tracked})"
        );
        assert!(
            in_reach(&on, &weapon, unit, target)?,
            "{unit_type:?}: premise: the chase reached its target"
        );
    }
    Ok(())
}

/// T-A6's scene: a carrier of `unit_type` walks east under a Move from (10, 30) to (20, 30), hull
/// and turret facing east, past a durable idle worker A at (17, 35), which comes into reach on
/// the way and is in reach where the Move ends, behind the hull's quarter (a bearing of about
/// 121° from where the Move ends); its flag is cleared in this world's catalog
/// copy unless `track`. Returns the world, the carrier and A.
fn pre_aim_scene(
    unit_type: UnitTypeId,
    track: bool,
) -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
    let mut world = shipped_world()?;
    let flags = carrier_weapon(&world, unit_type)?.target_flags;
    if !track {
        edit_weapons(&mut world, unit_type, |w| w.turret_tracks_any_enemy = false)?;
    }
    let worker_type = worker(&world, flags)?;
    let unit = spawn_facing(&mut world, unit_type, point(10, 30), SimAngle::ZERO)?;
    let enemy = world.spawn_unit(worker_type, PlayerId(1), point(17, 35))?;
    let durable = world.entities.get_mut(&enemy).ok_or("enemy")?;
    durable.max_hp = SimScalar::from_i32(100_000);
    durable.hp = durable.max_hp;
    let walk = UnitOrder::Move {
        target: point(20, 30),
    };
    step(&mut world, 0, vec![order(0, unit, walk)]);
    Ok((world, unit, enemy))
}

/// Whether `unit` is winding up in `world`.
fn winding_up(world: &SimWorld, unit: EntityId) -> bool {
    world
        .entities
        .get(&unit)
        .is_some_and(|e| matches!(e.weapon_phase, WeaponPhase::Windup { .. }))
}

/// T-A6 of Amendment 2 (class P with the shipped narrow arcs, A2.2): each carrier tracks worker A
/// while it walks, and its Move ends with A in reach. Its first swing then starts no later than
/// the flag-off twin's, and strictly earlier when the twin's turret lies more than one tick's
/// turn outside its swing arc of A on the tick the Move ends (a ready weapon turns and swings on
/// the same tick when one turn brings A within its acquisition arc, so a smaller error costs the
/// twin no tick; the scene places A so that it lies farther for both shipped carriers). A third world, tracking, whose turret is set to the
/// twin's on that tick equals the twin on every later tick: the turret's angle is the only
/// channel from tracking to the real weapon.
#[test]
fn a_carrier_pre_aimed_while_walking_swings_no_later() -> TestResult {
    for unit_type in walking_carriers(&shipped_world()?)? {
        let (mut on, unit, enemy) = pre_aim_scene(unit_type, true)?;
        let (mut off, ..) = pre_aim_scene(unit_type, false)?;
        let (mut equalized, ..) = pre_aim_scene(unit_type, true)?;
        let weapon = carrier_weapon(&on, unit_type)?;
        let mut ended = None;
        for tick in 1..300 {
            step(&mut on, tick, Vec::new());
            step(&mut off, tick, Vec::new());
            step(&mut equalized, tick, Vec::new());
            assert_twins(&on, &off, tick);
            let entity = on.entities.get(&unit).ok_or("unit")?;
            if !matches!(entity.order_queue.front(), Some(UnitOrder::Move { .. })) {
                ended = Some(tick);
                break;
            }
        }
        let ended = ended.ok_or("premise: the Move ended")?;
        assert!(
            in_reach(&on, &weapon, unit, enemy)?,
            "{unit_type:?}: premise: A is in reach where the Move ends"
        );
        let towards = bearing(&on, unit, enemy)?;
        let twin_turret = off.entities.get(&unit).ok_or("twin")?.turret_angle;
        let tracked_turret = on.entities.get(&unit).ok_or("unit")?.turret_angle;
        assert!(
            tracked_turret.diff(towards).unsigned_abs() < twin_turret.diff(towards).unsigned_abs(),
            "{unit_type:?}: premise: the tracking turret is pre-aimed at A"
        );
        let strictly = u32::from(twin_turret.diff(towards).unsigned_abs())
            > u32::from(weapon.turret_turn_rate.raw()) + u32::from(weapon.acquire_arc.raw());
        assert!(
            strictly,
            "{unit_type:?}: premise: the twin's turret needs more than one turn to bring A into \
             its arc"
        );
        equalized
            .entities
            .get_mut(&unit)
            .ok_or("unit")?
            .turret_angle = twin_turret;
        let (mut on_swing, mut off_swing) = (None, None);
        for tick in ended + 1..ended + 120 {
            step(&mut on, tick, Vec::new());
            step(&mut off, tick, Vec::new());
            step(&mut equalized, tick, Vec::new());
            assert_twins(&equalized, &off, tick);
            assert_eq!(
                equalized.entities.get(&unit).ok_or("unit")?.turret_angle,
                off.entities.get(&unit).ok_or("twin")?.turret_angle,
                "{unit_type:?} tick {tick}: with equal turrets the worlds stay equal"
            );
            if on_swing.is_none() && winding_up(&on, unit) {
                on_swing = Some(tick);
            }
            if off_swing.is_none() && winding_up(&off, unit) {
                off_swing = Some(tick);
            }
        }
        let on_swing = on_swing.ok_or("the carrier swings")?;
        let off_swing = off_swing.ok_or("the twin swings")?;
        assert!(
            on_swing <= off_swing,
            "{unit_type:?}: the pre-aimed swing starts no later ({on_swing} vs {off_swing})"
        );
        assert!(
            on_swing < off_swing,
            "{unit_type:?}: the pre-aimed swing starts earlier ({on_swing} vs {off_swing})"
        );
    }
    Ok(())
}

/// T-A7 of Amendment 2 (D-3 step 4, A2.1 S17): a walking carrier ranks the enemies it may track
/// by its archetype's tie-break. A worker N stands nearer than a worker F, and the carrier's
/// previous target stood next to F: a carrier that breaks ties by the previous target's position
/// (`ClosestToPrevious`, the Colossus analogue) tracks F, one that breaks them by distance
/// (`Closest`, the Immortal analogue) tracks N, on every tick both are in reach.
#[test]
fn a_walking_carrier_ranks_by_its_tie_break() -> TestResult {
    let world = shipped_world()?;
    let mut breaks = std::collections::BTreeSet::new();
    for unit_type in walking_carriers(&world)? {
        let tie_break = world.catalog.get(unit_type)?.acquire_tie_break;
        let mut world = shipped_world()?;
        let weapon = carrier_weapon(&world, unit_type)?;
        let worker_type = worker(&world, weapon.target_flags)?;
        let unit = spawn_facing(&mut world, unit_type, point(20, 30), SimAngle::ZERO)?;
        let near = world.spawn_unit(worker_type, PlayerId(1), point(22, 32))?;
        let far = world.spawn_unit(worker_type, PlayerId(1), point(17, 34))?;
        // As if its last target had stood next to F.
        world
            .entities
            .get_mut(&unit)
            .ok_or("unit")?
            .previous_target_position = Some(point(16, 35));
        step(
            &mut world,
            0,
            vec![order(
                0,
                unit,
                UnitOrder::Move {
                    target: point(30, 30),
                },
            )],
        );
        let expected = match tie_break {
            sim_core::catalog::AcquireTieBreak::ClosestToPrevious => far,
            sim_core::catalog::AcquireTieBreak::Closest => near,
            sim_core::catalog::AcquireTieBreak::LeastTurning => continue,
        };
        let mut both = 0;
        for tick in 1..120 {
            let reach = (
                in_reach(&world, &weapon, unit, near)?,
                in_reach(&world, &weapon, unit, far)?,
            );
            let nearer = {
                let at = world.entities.get(&unit).ok_or("unit")?.position;
                let n = world.entities.get(&near).ok_or("near")?.position;
                let f = world.entities.get(&far).ok_or("far")?.position;
                at.distance_sq(n) < at.distance_sq(f)
            };
            step(&mut world, tick, Vec::new());
            let walking = world
                .entities
                .get(&unit)
                .is_some_and(|e| matches!(e.order_queue.front(), Some(UnitOrder::Move { .. })));
            if !walking {
                break;
            }
            if reach == (true, true) && nearer {
                assert_eq!(
                    world.tracked_target(unit),
                    Some(expected),
                    "{unit_type:?} ({tie_break:?}) tick {tick}: ranked by the tie-break"
                );
                both += 1;
            }
        }
        assert!(
            both > 5,
            "{unit_type:?}: premise: both workers were in reach, N nearer ({both})"
        );
        breaks.insert(format!("{tie_break:?}"));
    }
    assert!(
        breaks.len() >= 2,
        "premise: carriers with both tie-breaks are shipped: {breaks:?}"
    );
    Ok(())
}
