//! # Integration Test: Fire Rules (Locked Turrets, Targets Off the Attack Order)
//!
//! Plan `2026-11-24-fire-rules` turns three `StarCraft` II 5.0.14 weapon options into rules over
//! catalog data:
//! - **The locked turret** (B-179, D-2): a weapon with `locks_turret_while_firing` (5.0.14's
//!   `LockTurretWhileFiring`, the Hellion's `InfernalFlameThrower`) does not turn its turret from
//!   the start of a swing to its release. The release and the damage are unchanged: the release
//!   checks the engagement band, not the arc, and the flame runs from the attacker to its target.
//! - **Targets off the attack order** (B-180, D-4, D-5): a weapon with `fires_at_other_targets`
//!   (5.0.14's `OnlyFireAtAttackOrderTarget 0`: the Phoenix, Immortal, Colossus and Planetary
//!   Fortress) works against its attack order's target whenever it can, and otherwise against
//!   the best enemy it reaches from where its unit is (`Entity::weapon_target`), ranked as
//!   acquisition ranks; a weapon that is not `Moving` does so only while its unit is not
//!   chasing. Its unit's order and its chase stay on the order's target.
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
    play_lock_scene_with(ticks, |weapon| weapon.locks_turret_while_firing = lock)
}

/// [`play_lock_scene`] with the locking weapon changed by `edit` in the world's catalog.
fn play_lock_scene_with(
    ticks: u32,
    edit: impl Fn(&mut WeaponSpec),
) -> Result<Vec<Frame>, Box<dyn std::error::Error>> {
    let mut world = shipped_world()?;
    let attacker_type = find(&world, MoverType::Ground, |weapon| {
        weapon.has_turret && weapon.locks_turret_while_firing
    })?;
    edit_weapons(&mut world, attacker_type, edit)?;
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

// ---------------------------------------------------------------------------------------------
// Targets off the attack order (B-180, D-4, D-5)
// ---------------------------------------------------------------------------------------------

/// The next free archetype identity.
fn free_type(world: &SimWorld) -> UnitTypeId {
    UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|arch| arch.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    )
}

/// Registers a scratch copy, under a fresh identity, of the shipped mobile archetype of lowest
/// type id on `mover` whose turreted weapon may fire at targets other than its attack order's,
/// with every own weapon changed by `edit`. Returns the identity and the edited weapon.
fn scratch_shooter(
    world: &mut SimWorld,
    mover: MoverType,
    edit: impl Fn(&mut WeaponSpec),
) -> Result<(UnitTypeId, WeaponSpec), Box<dyn std::error::Error>> {
    let source = find(world, mover, |weapon| {
        weapon.has_turret && weapon.fires_at_other_targets
    })?;
    let mut arch = world.catalog.get(source)?.clone();
    arch.unit_type_id = free_type(world);
    "scratch_fire_rules_shooter".clone_into(&mut arch.slug);
    for weapon in weapons_mut(&mut arch) {
        edit(weapon);
    }
    let weapon = weapons(&arch)
        .find(|weapon| weapon.has_turret)
        .ok_or("the shooter's turret")?
        .clone();
    let id = arch.unit_type_id;
    world.catalog.register(arch);
    Ok((id, weapon))
}

/// Registers an unarmed scratch copy, under a fresh identity, of the shipped mobile archetype of
/// lowest type id on `mover`, with a deep store of life, attack target priority `priority`, and
/// its speed scaled by `speed` (a ratio). Returns its identity.
fn scratch_dummy(
    world: &mut SimWorld,
    mover: MoverType,
    priority: u8,
    speed: i32,
) -> Result<UnitTypeId, Box<dyn std::error::Error>> {
    let mut arch = world
        .catalog
        .archetypes()
        .filter(|arch| arch.mover_type == mover && arch.max_speed > SimScalar::ZERO)
        .min_by_key(|arch| arch.unit_type_id)
        .cloned()
        .ok_or("the shipped catalog has a mobile archetype on the plane")?;
    arch.unit_type_id = free_type(world);
    format!("scratch_fire_rules_dummy_{priority}_{speed}").clone_into(&mut arch.slug);
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.abilities.clear();
    arch.max_hp = SimScalar::from_i32(5000);
    arch.attack_target_priority = priority;
    arch.max_speed *= SimScalar::from_i32(speed);
    arch.acceleration *= SimScalar::from_i32(speed);
    let id = arch.unit_type_id;
    world.catalog.register(arch);
    Ok(id)
}

/// Puts `id` on Hold Position, so that a struck dummy stands rather than flees.
fn hold(world: &mut SimWorld, id: EntityId) -> TestResult {
    world
        .entities
        .get_mut(&id)
        .ok_or("the unit exists")?
        .issue_order(UnitOrder::HoldPosition, false)
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}

/// Life and shields of `id`, or zeros once it is gone.
fn life(world: &SimWorld, id: EntityId) -> (SimScalar, SimScalar) {
    world
        .entities
        .get(&id)
        .map_or((SimScalar::ZERO, SimScalar::ZERO), |e| (e.hp, e.shields))
}

/// The target of `id`'s swing, if it is winding up.
fn swing_target(world: &SimWorld, id: EntityId) -> Option<EntityId> {
    match world.entities.get(&id).map(|e| &e.weapon_phase) {
        Some(WeaponPhase::Windup { target_id, .. }) => Some(*target_id),
        _ => None,
    }
}

/// P-3: a unit whose weapon may fire at other targets, under a player's attack on a target in
/// its band, fires only at that target while a second enemy of higher rank stands in its reach.
#[test]
fn the_order_target_comes_first() -> TestResult {
    let mut world = shipped_world()?;
    let (shooter_type, weapon) = scratch_shooter(&mut world, MoverType::Ground, |_| {})?;
    let low = scratch_dummy(&mut world, MoverType::Ground, 10, 1)?;
    let high = scratch_dummy(&mut world, MoverType::Ground, 250, 1)?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), point(20, 20))?;
    world
        .entities
        .get_mut(&shooter)
        .ok_or("shooter")?
        .turret_angle = SimAngle::ZERO;
    let target = world.spawn_unit(low, PlayerId(1), point(24, 20))?;
    let other = world.spawn_unit(high, PlayerId(1), point(20, 24))?;
    hold(&mut world, target)?;
    hold(&mut world, other)?;
    assert!(
        weapon.max_range >= SimScalar::from_i32(4),
        "premise: both enemies stand in reach"
    );
    let untouched = life(&world, other);
    let full = life(&world, target);
    step(&mut world, 0, vec![order(0, shooter, attack(target))]);
    for tick in 1..120 {
        step(&mut world, tick, Vec::new());
        if let Some(swing) = swing_target(&world, shooter) {
            assert_eq!(
                swing, target,
                "tick {tick}: every swing is at the order's target"
            );
        }
        let unit = world.entities.get(&shooter).ok_or("shooter")?;
        assert_eq!(
            unit.weapon_target, None,
            "tick {tick}: no own target is chosen"
        );
    }
    assert_eq!(
        life(&world, other),
        untouched,
        "the higher-ranked enemy is spared"
    );
    assert_ne!(life(&world, target), full, "the order's target is struck");
    Ok(())
}

/// The chase scene of P-4, P-5 and P-10: a scratch air shooter (its weapon edited by `edit`) at
/// (10, 30) under a player's attack on an air enemy at (17, 30) that flees east at twice its
/// speed, past a second air enemy holding at (20, 31). Returns the damage the passed enemy took
/// and whether the shooter ever swung at it; on every tick the shooter's order and its pursuit
/// target stay on the fleeing enemy.
fn chase_scene(
    edit: impl Fn(&mut WeaponSpec),
) -> Result<(SimScalar, bool), Box<dyn std::error::Error>> {
    let mut world = shipped_world()?;
    let (shooter_type, _) = scratch_shooter(&mut world, MoverType::Air, edit)?;
    let fleeing_type = scratch_dummy(&mut world, MoverType::Air, 20, 2)?;
    let passed_type = scratch_dummy(&mut world, MoverType::Air, 20, 1)?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), point(10, 30))?;
    let fleeing = world.spawn_unit(fleeing_type, PlayerId(1), point(17, 30))?;
    let passed = world.spawn_unit(passed_type, PlayerId(1), point(20, 31))?;
    let before = life(&world, passed);
    step(
        &mut world,
        0,
        vec![
            order(0, shooter, attack(fleeing)),
            order(
                1,
                fleeing,
                UnitOrder::Move {
                    target: point(62, 30),
                },
            ),
            order(1, passed, UnitOrder::HoldPosition),
        ],
    );
    let mut swung = false;
    for tick in 1..80 {
        step(&mut world, tick, Vec::new());
        swung |= swing_target(&world, shooter) == Some(passed);
        let unit = world.entities.get(&shooter).ok_or("shooter")?;
        assert_eq!(
            unit.order_queue.front(),
            Some(&attack(fleeing)),
            "tick {tick}: the order stays on the fleeing enemy"
        );
        assert_eq!(
            unit.current_target,
            Some(fleeing),
            "tick {tick}: pursuit chases the fleeing enemy"
        );
    }
    let after = life(&world, passed);
    Ok(((before.0 + before.1) - (after.0 + after.1), swung))
}

/// P-4: a weapon that fires while moving and may fire at other targets strikes an enemy it
/// passes in reach while it chases its order's target; without the option it strikes nothing.
#[test]
fn a_weapon_that_fires_while_moving_strikes_what_it_passes_in_a_chase() -> TestResult {
    let moving = |weapon: &mut WeaponSpec| {
        weapon.allowed_movement = sim_types::AllowedMovement::Moving;
    };
    let (damage, swung) = chase_scene(moving)?;
    assert!(swung, "the shooter swings at the enemy it passes");
    assert!(damage > SimScalar::ZERO, "the passed enemy is struck");
    let (damage, swung) = chase_scene(|weapon| {
        moving(weapon);
        weapon.fires_at_other_targets = false;
    })?;
    assert!(!swung, "without the option the shooter never swings at it");
    assert_eq!(
        damage,
        SimScalar::ZERO,
        "without the option nothing is struck"
    );
    Ok(())
}

/// P-5: a weapon that stands to fire (`Stopped` or `Slowing`) holds fire at an enemy it passes
/// while it chases its order's target, though it may fire at other targets.
#[test]
fn a_weapon_that_stands_to_fire_holds_in_a_chase() -> TestResult {
    for movement in [
        sim_types::AllowedMovement::Stopped,
        sim_types::AllowedMovement::Slowing,
    ] {
        let (damage, swung) = chase_scene(|weapon| {
            weapon.allowed_movement = movement;
            weapon.fires_at_other_targets = true;
        })?;
        assert!(!swung, "{movement:?}: no swing at the passed enemy");
        assert_eq!(damage, SimScalar::ZERO, "{movement:?}: nothing is struck");
    }
    Ok(())
}

/// The vision scene of P-6 and P-9: a flat 48 × 48 world with a scratch ground shooter (its
/// weapon `Stopped`, its reach widened to its sight plus 6, its archetype's tie-break `tie`) at
/// the centre of cell (10, 24), under a player's attack on a dummy of player 1 four cells beyond
/// its sight in its row: in reach, unseen.
struct VisionScene {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
    sight_cells: u32,
}

impl VisionScene {
    fn new(tie: sim_core::catalog::AcquireTieBreak) -> Result<Self, Box<dyn std::error::Error>> {
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = sim_fixtures::world(map, 0xF1E);
        let source = find(&world, MoverType::Ground, |weapon| {
            weapon.has_turret && weapon.fires_at_other_targets
        })?;
        let sight = world.catalog.get(source)?.sight_radius;
        let (shooter_type, _) = scratch_shooter(&mut world, MoverType::Ground, |weapon| {
            weapon.allowed_movement = sim_types::AllowedMovement::Stopped;
            weapon.max_range = sight + SimScalar::from_i32(6);
            weapon.min_range = SimScalar::ZERO;
        })?;
        let arch = world.catalog.get_mut(shooter_type)?;
        arch.acquire_tie_break = tie;
        arch.detection_radius = None;
        let sight_cells = sight.saturating_to_u32();
        let dummy = scratch_dummy(&mut world, MoverType::Ground, 20, 1)?;
        let shooter = world.spawn_unit(shooter_type, PlayerId(0), Self::cell(10, 24))?;
        let target = world.spawn_unit(dummy, PlayerId(1), Self::cell(10 + sight_cells + 4, 24))?;
        hold(&mut world, target)?;
        step(&mut world, 0, vec![order(0, shooter, attack(target))]);
        Ok(Self {
            world,
            shooter,
            target,
            sight_cells,
        })
    }

    /// The centre of cell `(x, y)`.
    fn cell(x: u32, y: u32) -> SimVec2 {
        SimVec2::new(
            SimScalar::from_num(f64::from(x) + 0.5),
            SimScalar::from_num(f64::from(y) + 0.5),
        )
    }

    /// Spawns a dummy of `owner` with attack target priority `priority` at `at`.
    fn dummy(
        &mut self,
        owner: u8,
        priority: u8,
        at: SimVec2,
    ) -> Result<EntityId, Box<dyn std::error::Error>> {
        let kind = scratch_dummy(&mut self.world, MoverType::Ground, priority, 1)?;
        let id = self.world.spawn_unit(kind, PlayerId(owner), at)?;
        hold(&mut self.world, id)?;
        Ok(id)
    }

    fn shooter(&self) -> Result<&sim_core::Entity, Box<dyn std::error::Error>> {
        Ok(self.world.entities.get(&self.shooter).ok_or("shooter")?)
    }
}

/// P-6: a unit whose `Stopped` weapon may fire at other targets, under a player's attack on an
/// enemy in reach that its player does not see, stands and fires at a seen enemy in reach; it
/// returns to the order's target on the tick that target is seen again.
#[test]
fn a_standing_unit_fires_at_a_seen_enemy_while_its_target_is_unseen() -> TestResult {
    let mut scene = VisionScene::new(sim_core::catalog::AcquireTieBreak::Closest)?;
    let seen = scene.dummy(1, 20, VisionScene::cell(10 + scene.sight_cells - 2, 26))?;
    let start = scene.shooter()?.position;
    let full = life(&scene.world, seen);
    let mut tick = 1;
    while life(&scene.world, seen) == full {
        assert!(tick < 150, "the seen enemy is struck within 150 ticks");
        step(&mut scene.world, tick, Vec::new());
        assert_eq!(
            scene.shooter()?.position,
            start,
            "tick {tick}: the unit stands"
        );
        tick += 1;
    }
    assert_eq!(scene.shooter()?.weapon_target, Some(seen));
    assert_eq!(
        scene.shooter()?.order_queue.front(),
        Some(&attack(scene.target)),
        "the player's order is kept"
    );
    // A spotter of player 0 beside the order's target: it is seen from the next tick's vision.
    let unseen_full = life(&scene.world, scene.target);
    let at = VisionScene::cell(10 + scene.sight_cells + 4, 22);
    scene.dummy(0, 20, at)?;
    let seen_again = tick;
    let mut previous = scene.shooter()?.weapon_phase.clone();
    for tick in seen_again..seen_again + 120 {
        step(&mut scene.world, tick, Vec::new());
        let unit = scene.shooter()?;
        assert_eq!(
            unit.weapon_target, None,
            "tick {tick}: the own target is released"
        );
        if !matches!(previous, WeaponPhase::Windup { .. }) {
            if let WeaponPhase::Windup { target_id, .. } = unit.weapon_phase {
                assert_eq!(
                    target_id, scene.target,
                    "tick {tick}: a new swing is at the order's target"
                );
            }
        }
        previous = unit.weapon_phase.clone();
    }
    assert_ne!(
        life(&scene.world, scene.target),
        unseen_full,
        "the order's target is struck once seen"
    );
    Ok(())
}

/// P-9: the own target is ranked as acquisition ranks: the higher attack target priority first;
/// among equals, the archetype's tie-break, `ClosestToPrevious` measured from the order's target.
#[test]
fn its_own_target_is_ranked_as_acquisition_ranks() -> TestResult {
    use sim_core::catalog::AcquireTieBreak;
    let near = |scene: &VisionScene| VisionScene::cell(10 + scene.sight_cells - 6, 24);
    let far = |scene: &VisionScene| VisionScene::cell(10 + scene.sight_cells - 1, 23);
    let settle = |scene: &mut VisionScene| {
        for tick in 1..6 {
            step(&mut scene.world, tick, Vec::new());
        }
    };
    // Priority beats distance.
    let mut scene = VisionScene::new(AcquireTieBreak::Closest)?;
    scene.dummy(1, 20, near(&scene))?;
    let ranked = scene.dummy(1, 40, far(&scene))?;
    settle(&mut scene);
    assert_eq!(
        scene.shooter()?.weapon_target,
        Some(ranked),
        "the higher priority"
    );
    // Equal priority: the closest to the unit.
    let mut scene = VisionScene::new(AcquireTieBreak::Closest)?;
    let closest = scene.dummy(1, 20, near(&scene))?;
    scene.dummy(1, 20, far(&scene))?;
    settle(&mut scene);
    assert_eq!(scene.shooter()?.weapon_target, Some(closest), "the closest");
    // Equal priority, `ClosestToPrevious`: the closest to the order's target.
    let mut scene = VisionScene::new(AcquireTieBreak::ClosestToPrevious)?;
    scene.dummy(1, 20, near(&scene))?;
    let by_target = scene.dummy(1, 20, far(&scene))?;
    settle(&mut scene);
    assert_eq!(
        scene.shooter()?.weapon_target,
        Some(by_target),
        "the closest to the order's target"
    );
    Ok(())
}

/// P-7: an immobile unit whose weapon may fire at other targets, under a player's attack on a
/// seen enemy in reach, never fires at a higher-ranked enemy in reach, even while its turret
/// still turns towards the order's target; its own target is never set.
#[test]
fn an_immobile_unit_never_leaves_a_workable_order_target() -> TestResult {
    let mut world = shipped_world()?;
    let (shooter_type, _) = scratch_shooter(&mut world, MoverType::Ground, |_| {})?;
    let low = scratch_dummy(&mut world, MoverType::Ground, 10, 1)?;
    let high = scratch_dummy(&mut world, MoverType::Ground, 250, 1)?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), point(20, 20))?;
    let unit = world.entities.get_mut(&shooter).ok_or("shooter")?;
    unit.is_immobile = true;
    unit.turret_angle = SimAngle::DEG_180;
    let target = world.spawn_unit(low, PlayerId(1), point(24, 20))?;
    let other = world.spawn_unit(high, PlayerId(1), point(16, 20))?;
    hold(&mut world, target)?;
    hold(&mut world, other)?;
    let untouched = life(&world, other);
    step(&mut world, 0, vec![order(0, shooter, attack(target))]);
    for tick in 1..100 {
        step(&mut world, tick, Vec::new());
        assert_ne!(swing_target(&world, shooter), Some(other), "tick {tick}");
        let unit = world.entities.get(&shooter).ok_or("shooter")?;
        assert_eq!(unit.weapon_target, None, "tick {tick}: no own target");
    }
    assert_eq!(life(&world, other), untouched, "the other enemy is spared");
    Ok(())
}

/// P-7: an immobile unit's attack on an enemy out of its reach, or in reach and unseen, ends at
/// once (rule 6, Q0109 part 4's default), and no own target is set for it.
#[test]
fn an_immobile_units_order_on_an_unworkable_target_ends() -> TestResult {
    // Out of reach.
    let mut world = shipped_world()?;
    let (shooter_type, weapon) = scratch_shooter(&mut world, MoverType::Ground, |_| {})?;
    let dummy = scratch_dummy(&mut world, MoverType::Ground, 10, 1)?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), point(20, 20))?;
    world
        .entities
        .get_mut(&shooter)
        .ok_or("shooter")?
        .is_immobile = true;
    let beyond = weapon.max_range.saturating_to_u32() + 4;
    let far = world.spawn_unit(dummy, PlayerId(1), point(20 + i32::try_from(beyond)?, 20))?;
    world.spawn_unit(dummy, PlayerId(1), point(23, 20))?;
    step(&mut world, 0, vec![order(0, shooter, attack(far))]);
    step(&mut world, 1, Vec::new());
    let unit = world.entities.get(&shooter).ok_or("shooter")?;
    assert_ne!(
        unit.order_queue.front(),
        Some(&attack(far)),
        "the order ends"
    );
    assert_eq!(unit.weapon_target, None);
    // In reach, unseen.
    let mut scene = VisionScene::new(sim_core::catalog::AcquireTieBreak::Closest)?;
    scene
        .world
        .entities
        .get_mut(&scene.shooter)
        .ok_or("shooter")?
        .is_immobile = true;
    scene.dummy(1, 20, VisionScene::cell(10 + scene.sight_cells - 2, 26))?;
    step(
        &mut scene.world,
        1,
        vec![order(0, scene.shooter, attack(scene.target))],
    );
    step(&mut scene.world, 2, Vec::new());
    let unit = scene.shooter()?;
    assert_ne!(
        unit.order_queue.front(),
        Some(&attack(scene.target)),
        "the order ends"
    );
    assert_eq!(unit.weapon_target, None);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Turrets between shots (B-197, D-3) and the lock's backswing half (D-2)
// ---------------------------------------------------------------------------------------------

/// The first tick at or after `from` whose phase `pick` chooses.
fn first_from(
    frames: &[Frame],
    from: usize,
    pick: impl Fn(&WeaponPhase) -> bool,
) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(from
        + frames[from..]
            .iter()
            .position(|frame| pick(&frame.phase))
            .ok_or("the phase never comes")?)
}

/// P-2: a turret that locks while firing also holds through its backswing; once the backswing
/// ends, it follows its target through the rest of the cooldown, as every turret does.
#[test]
fn a_locked_turret_holds_through_its_backswing() -> TestResult {
    let frames = play_lock_scene(true, 60)?;
    let (start, release) = first_swing(&frames)?;
    let backswing_end = first_from(&frames, release, |phase| {
        !matches!(phase, WeaponPhase::Backswing { .. })
    })?;
    assert!(
        matches!(frames[release].phase, WeaponPhase::Backswing { .. }),
        "premise: the weapon follows through"
    );
    for tick in start..backswing_end {
        assert_eq!(
            frames[tick].turret, frames[start].turret,
            "tick {tick}: the locked turret holds from the swing's start ({start}) to the end \
             of its backswing ({backswing_end})"
        );
    }
    assert_eq!(frames[backswing_end].phase, WeaponPhase::Cooldown);
    let next = first_from(&frames, backswing_end, |phase| {
        !matches!(phase, WeaponPhase::Cooldown)
    })?;
    assert!(
        (backswing_end..next).any(|tick| frames[tick].turret != frames[start].turret),
        "the turret follows its target in the cooldown ({backswing_end} to {next})"
    );
    Ok(())
}

/// P-2: a swing that starts out of the backswing (here a backswing longer than the cooldown, in
/// the world's copy of the catalog) ends the lock: the turret turns on its first tick.
#[test]
fn a_swing_out_of_the_backswing_turns_it() -> TestResult {
    let frames = play_lock_scene_with(80, |weapon| {
        weapon.locks_turret_while_firing = true;
        weapon.backswing_ticks = weapon.period_ticks + 10;
    })?;
    let (start, release) = first_swing(&frames)?;
    let out = first_from(&frames, release, |phase| {
        !matches!(phase, WeaponPhase::Backswing { .. })
    })?;
    assert!(
        matches!(
            frames[out].phase,
            WeaponPhase::Windup { .. } | WeaponPhase::RotatingToTarget { .. }
        ),
        "premise: the next swing starts out of the backswing: {:?}",
        frames[out].phase
    );
    for tick in start..out {
        assert_eq!(
            frames[tick].turret, frames[start].turret,
            "tick {tick}: locked"
        );
    }
    assert_ne!(
        frames[out].turret,
        frames[out - 1].turret,
        "tick {out}: the new swing turns the turret on its first tick"
    );
    Ok(())
}

/// P-13: a turreted weapon whose engaged arc is narrower than its target's change of bearing
/// during the cooldown starts its next swing on the tick its cooldown ends, its turret having
/// followed the target between shots: it never rotates after the cooldown.
#[test]
fn the_next_swing_starts_no_later() -> TestResult {
    let mut world = shipped_world()?;
    let shooter_type = find(&world, MoverType::Ground, |weapon| {
        weapon.has_turret
            && !weapon.locks_turret_while_firing
            && weapon.firing_arc < SimAngle::from_degrees(20)
    })?;
    edit_weapons(&mut world, shooter_type, |weapon| {
        weapon.random_delay_min = SimScalar::ZERO;
        weapon.random_delay_max = SimScalar::ZERO;
    })?;
    let walker = scratch_dummy(&mut world, MoverType::Ground, 20, 1)?;
    let shooter = spawn_facing_south(&mut world, shooter_type, 0, point(20, 20))?;
    // The target passes 2 south of the shooter, east to west: its bearing turns by more than
    // the turret's rate plus its arc during one cooldown.
    let target = world.spawn_unit(walker, PlayerId(1), point(23, 22))?;
    step(
        &mut world,
        0,
        vec![
            order(0, shooter, attack(target)),
            order(
                1,
                target,
                UnitOrder::Move {
                    target: point(8, 22),
                },
            ),
        ],
    );
    let mut frames = Vec::new();
    let mut cooldowns = Vec::new();
    for tick in 1..60 {
        step(&mut world, tick, Vec::new());
        frames.push(Frame::of(&world, shooter)?);
        let unit = world.entities.get(&shooter).ok_or("shooter")?;
        cooldowns.push(unit.weapon_cooldown_remaining);
    }
    let (_, release) = first_swing(&frames)?;
    let next = first_from(&frames, release, |phase| {
        matches!(phase, WeaponPhase::Windup { .. })
    })?;
    let ready = release
        + cooldowns[release..]
            .iter()
            .position(|&left| left <= 1)
            .ok_or("the cooldown ends")?
        + 1;
    let rate = weapons(world.catalog.get(shooter_type)?)
        .find(|weapon| weapon.has_turret)
        .map(|weapon| (weapon.turret_turn_rate, weapon.firing_arc))
        .ok_or("the turret")?;
    let turned = frames[release]
        .turret
        .diff(frames[next].turret)
        .unsigned_abs();
    assert!(
        turned > rate.0.raw().saturating_add(rate.1.raw()),
        "premise: the bearing turned by more than one tick's turn plus the arc between the \
         shots ({turned})"
    );
    assert!(
        !frames[release..next]
            .iter()
            .any(|frame| matches!(frame.phase, WeaponPhase::RotatingToTarget { .. })),
        "the weapon never rotates after its cooldown"
    );
    assert_eq!(
        next, ready,
        "the next swing starts on the tick the cooldown ends"
    );
    Ok(())
}
