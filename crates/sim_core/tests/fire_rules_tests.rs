//! # Integration Test: Fire Rules (Locked Turrets, Targets Off the Attack Order)
//!
//! Plan `2026-11-24-fire-rules` turns three `StarCraft` II 5.0.14 weapon options into rules over
//! catalog data:
//! - **The locked turret** (B-179, D-2): a weapon with `locks_turret_while_firing` (5.0.14's
//!   `LockTurretWhileFiring`, the Hellion's `InfernalFlameThrower`) does not turn its turret from
//!   the start of a swing to its release. The release and the damage are unchanged: the release
//!   checks the engagement band, not the arc, and the flame runs from the attacker to its target.
//!
//! These tests play the shipped catalog and find their units by property, never by name. Where a
//! test compares a rule on and off, it plays two worlds that differ only in one weapon field of a
//! scratch copy of the archetype (`SimWorld::catalog`), so every other value stays the shipped
//! data. Positions are world units; angles are `SimAngle` binary angles; times are ticks.

use sim_core::catalog::UnitArchetype;
use sim_core::{SimWorld, WeaponPhase, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, MoverType, PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
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

/// The weapons in an archetype's own slots, mutably.
fn weapons_mut(archetype: &mut UnitArchetype) -> impl Iterator<Item = &mut WeaponSpec> {
    [
        archetype.ground_weapon.as_mut(),
        archetype.air_weapon.as_mut(),
        archetype.weapon.as_mut(),
    ]
    .into_iter()
    .flatten()
}

/// The shipped mobile archetype of lowest type id on `mover` with a weapon `pick` chooses.
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

/// The shipped mobile archetype of lowest type id on `mover` that `weapon` may strike, with at
/// least `life` hit points and shields together: a target that outlives the shots a test needs.
fn target_for(
    world: &SimWorld,
    mover: MoverType,
    weapon: &WeaponSpec,
    life: i32,
) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == mover
                && archetype.max_speed > SimScalar::ZERO
                && weapon.target_flags.intersects(archetype.attributes)
                && archetype.max_hp + archetype.max_shields >= SimScalar::from_i32(life)
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("no shipped archetype is such a target")?
        .unit_type_id)
}

/// Sets the field `edit` changes on every own weapon of `unit_type` in `world`'s catalog, which
/// is the world's scratch copy of the shipped data.
fn edit_weapons(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
    edit: impl Fn(&mut WeaponSpec),
) -> TestResult {
    for weapon in weapons_mut(world.catalog.get_mut(unit_type)?) {
        edit(weapon);
    }
    Ok(())
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

/// A player's attack on `target`.
fn attack(target: EntityId) -> UnitOrder {
    UnitOrder::AttackTarget {
        target,
        origin: AttackOrigin::Explicit,
    }
}

/// A unit of `unit_type` for `player` at `at` whose hull and turret face south (+y).
fn spawn_facing_south(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
    player: u8,
    at: SimVec2,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let unit = world.spawn_unit(unit_type, PlayerId(player), at)?;
    let entity = world.entities.get_mut(&unit).ok_or("unit")?;
    entity.heading = SimAngle::DEG_90;
    entity.turret_angle = SimAngle::DEG_90;
    Ok(unit)
}

/// What one tick of a played scene leaves: the attacker's weapon phase and turret angle, and the
/// life and shields of every entity.
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    phase: WeaponPhase,
    turret: SimAngle,
    life: Vec<(EntityId, SimScalar, SimScalar)>,
}

impl Frame {
    fn of(world: &SimWorld, attacker: EntityId) -> Result<Self, Box<dyn std::error::Error>> {
        let unit = world.entities.get(&attacker).ok_or("attacker")?;
        Ok(Self {
            phase: unit.weapon_phase.clone(),
            turret: unit.turret_angle,
            life: world
                .entities
                .iter()
                .map(|(id, entity)| (*id, entity.hp, entity.shields))
                .collect(),
        })
    }

    fn winding_up(&self) -> bool {
        matches!(self.phase, WeaponPhase::Windup { .. })
    }
}

/// The ticks of the first swing in `frames` (indexed by tick): the tick it started (the first
/// `Windup`) and the tick it released (the first tick after it that is not a `Windup`).
fn first_swing(frames: &[Frame]) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let start = frames
        .iter()
        .position(Frame::winding_up)
        .ok_or("the attacker never winds up")?;
    let release = start
        + frames[start..]
            .iter()
            .position(|frame| !frame.winding_up())
            .ok_or("the swing never releases")?;
    Ok((start, release))
}

/// Plays the lock scene: a unit with a turreted weapon that locks while firing (found by
/// property; with its lock set to `lock` in the world's catalog) stands at (20, 20) under a
/// player's attack on an enemy 4 south of it, which walks east across its front. Returns one
/// frame per tick, from tick 0, for `ticks` ticks.
fn play_lock_scene(lock: bool, ticks: u32) -> Result<Vec<Frame>, Box<dyn std::error::Error>> {
    let mut world = shipped_world()?;
    let attacker_type = find(&world, MoverType::Ground, |weapon| {
        weapon.has_turret && weapon.locks_turret_while_firing
    })?;
    edit_weapons(&mut world, attacker_type, |weapon| {
        weapon.locks_turret_while_firing = lock;
    })?;
    let weapon = weapons(world.catalog.get(attacker_type)?)
        .find(|weapon| weapon.has_turret)
        .ok_or("the attacker's turreted weapon")?
        .clone();
    let target_type = target_for(&world, MoverType::Ground, &weapon, 100)?;
    let attacker = spawn_facing_south(&mut world, attacker_type, 0, point(20, 20))?;
    let target = world.spawn_unit(target_type, PlayerId(1), point(20, 24))?;
    step(
        &mut world,
        0,
        vec![
            order(0, attacker, attack(target)),
            order(
                1,
                target,
                UnitOrder::Move {
                    target: point(40, 24),
                },
            ),
        ],
    );
    let mut frames = vec![Frame::of(&world, attacker)?];
    for tick in 1..ticks {
        step(&mut world, tick, Vec::new());
        frames.push(Frame::of(&world, attacker)?);
    }
    Ok(frames)
}

/// P-1: a turreted weapon that locks its turret keeps its turret angle on every tick from the
/// start of its swing to its release, while its target walks across its front.
#[test]
fn a_locked_turret_holds_through_its_wind_up() -> TestResult {
    let frames = play_lock_scene(true, 40)?;
    let (start, release) = first_swing(&frames)?;
    assert!(release > start + 1, "premise: the swing winds up");
    for tick in start..=release {
        assert_eq!(
            frames[tick].turret, frames[start].turret,
            "tick {tick}: the locked turret holds from the swing's start ({start}) to its \
             release ({release})"
        );
    }
    Ok(())
}

/// P-1: the same weapon without the lock turns its turret during the wind-up towards the target
/// walking across its front.
#[test]
fn an_unlocked_turret_turns_through_its_wind_up() -> TestResult {
    let frames = play_lock_scene(false, 40)?;
    let (start, release) = first_swing(&frames)?;
    assert!(
        (start + 1..=release).any(|tick| frames[tick].turret != frames[start].turret),
        "the unlocked turret turns between the swing's start ({start}) and its release \
         ({release})"
    );
    Ok(())
}

/// P-1: the lock changes neither when the shot releases nor what it strikes: every entity's life
/// and shields are equal on every tick up to two ticks after the release, with and without the
/// lock.
#[test]
fn the_lock_changes_neither_release_nor_damage() -> TestResult {
    let locked = play_lock_scene(true, 40)?;
    let free = play_lock_scene(false, 40)?;
    let (start, release) = first_swing(&locked)?;
    assert_eq!(
        first_swing(&free)?,
        (start, release),
        "the swing starts and releases on the same ticks"
    );
    let struck = locked[release + 2]
        .life
        .iter()
        .zip(&locked[0].life)
        .any(|(after, before)| after != before);
    assert!(struck, "premise: the shot struck something");
    for tick in 0..=release + 2 {
        assert_eq!(
            locked[tick].life, free[tick].life,
            "tick {tick}: the same damage lands on the same units"
        );
    }
    Ok(())
}
