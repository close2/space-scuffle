//! Missiles validate their target in flight, and weapons strike only what their player sees.
//!
//! # Responsibility
//! Pins plan `2026-11-24-weapon-validation` against `StarCraft` II 5.0.14:
//! - D-2 (P-3): a teleport is counted on the unit it relocates; nothing else counts one.
//! - D-4, B-182 (P-1): a missile whose target teleports after the launch (5.0.14's
//!   `ValidateTeleport 1`, `core` EffectData:100; Blink is a `CEffectTeleport`) loses it and,
//!   by `PointFallback`, flies on at its speed to the last point it tracked, where the target
//!   stood before the teleport; a single-target missile strikes nobody, an area strikes around
//!   the point. A missile launched after the teleport tracks the target normally.
//!
//! # How it fits
//! The teleport count (`Entity::teleport_count`) is written by `Entity::teleport_to`, which every
//! teleport effect goes through; projectiles record it at launch (`ActiveProjectile::
//! target_teleport_count`), and the combat phase compares the two in flight.
//!
//! # Invariants
//! - No unit is named: archetypes and weapons are found by their properties, and Blink is
//!   authored on a scratch copy of a shipped archetype, since no shipped unit carries it
//!   (charter §3.3).
//! - Every world is flat unless a test says otherwise.

use sim_core::{SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EntityId, MoverType, PlayerId, SimScalar,
    SimVec2, SplashCliff, SplashImpact, SplashShape, SplashZone, Tick, TickBundle, UnitOrder,
    UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn point(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Steps `world` from `first` through `last`, inclusive.
fn steps(world: &mut SimWorld, first: u32, last: u32) {
    for tick in first..=last {
        world.step(&bundle(tick));
    }
}

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

/// A Blink in the shape of 5.0.14's `Blink` (`CEffectTeleport`, `liberty` EffectData:2347–2356):
/// a point-targeted teleport, free and without cooldown so a test may cast it at will.
fn blink_ability() -> AbilitySpec {
    AbilitySpec {
        name: "scratch blink".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cast_range: SimScalar::from_num(30),
        cooldown_ticks: 0,
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_num(30),
            cooldown_ticks: 0,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }
}

/// Registers an unarmed scratch copy of a shipped mobile ground archetype whose only ability is
/// [`blink_ability`]; returns its identity.
fn register_blinker(world: &mut SimWorld) -> Result<UnitTypeId, String> {
    let mut arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
        .cloned()
        .ok_or("the shipped catalog has a mobile ground archetype")?;
    arch.unit_type_id = free_type(world);
    "scratch_blinker".clone_into(&mut arch.slug);
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.abilities = vec![blink_ability()];
    let id = arch.unit_type_id;
    world.catalog.register(arch);
    Ok(id)
}

/// A flat 32x32 world with one blinker of player 1 at (8, 8).
fn blink_world(seed: u64) -> Result<(SimWorld, EntityId), Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let kind = register_blinker(&mut world)?;
    let unit = world.spawn_unit(kind, PlayerId(1), point(8.0, 8.0))?;
    Ok((world, unit))
}

/// P-3: a Blink relocates the unit and counts one teleport.
#[test]
fn a_blink_counts_one_teleport() -> TestResult {
    let (mut world, unit) = blink_world(0xB10)?;
    assert_eq!(world.entities[&unit].teleport_count, 0);
    let destination = point(14.0, 8.0);
    assert!(
        world.execute_ability(unit, 0, Some(destination), None),
        "the blink is admitted"
    );
    assert_eq!(world.entities[&unit].position, destination);
    assert_eq!(world.entities[&unit].teleport_count, 1);
    assert!(world.execute_ability(unit, 0, Some(point(14.0, 14.0)), None));
    assert_eq!(world.entities[&unit].teleport_count, 2, "one per Blink");
    Ok(())
}

/// P-3: walking, boarding and leaving a transport relocate a unit without a teleport.
#[test]
fn walking_and_an_unload_count_no_teleport() -> TestResult {
    let (mut world, unit) = blink_world(0xB11)?;
    let kind = world.entities[&unit].unit_type_id;
    world
        .entities
        .get_mut(&unit)
        .ok_or("the unit exists")?
        .issue_order(
            UnitOrder::Move {
                target: point(20.0, 8.0),
            },
            false,
        )
        .map_err(|error| format!("{error:?}"))?;
    steps(&mut world, 1, 40);
    assert!(
        world.entities[&unit].position.x > SimScalar::from_num(9),
        "the unit walked"
    );
    assert_eq!(
        world.entities[&unit].teleport_count, 0,
        "a walk is no teleport"
    );
    let carrier = world.spawn_unit(kind, PlayerId(1), point(24.0, 24.0))?;
    world
        .try_load_unit_into_transport(unit, carrier)
        .map_err(|error| format!("{error:?}"))?;
    world
        .try_unload_unit_from_transport(unit, point(4.0, 24.0))
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(world.entities[&unit].position, point(4.0, 24.0));
    assert_eq!(
        world.entities[&unit].teleport_count, 0,
        "an unload is no teleport"
    );
    Ok(())
}

/// Casts every unit-targeted effect ability of every shipped archetype on the blinker, one fresh
/// world each, until one flies it (`Entity::displacement`, a `Displace` node); returns the
/// world once the flight has ended and the blinker, or `None` if no shipped ability displaces.
fn displaced_blinker() -> Result<Option<(SimWorld, EntityId)>, Box<dyn std::error::Error>> {
    let (probe, _) = blink_world(0xB12)?;
    let casts: Vec<(UnitTypeId, u8)> = probe
        .catalog
        .archetypes()
        .flat_map(|arch| {
            arch.abilities
                .iter()
                .enumerate()
                .filter(|(_, ability)| {
                    ability.target_kind == AbilityTargetKind::TargetEntity
                        && matches!(ability.effect, AbilityEffectSpec::Effect(_))
                })
                .filter_map(|(index, _)| u8::try_from(index).ok())
                .map(|index| (arch.unit_type_id, index))
                .collect::<Vec<_>>()
        })
        .collect();
    for (kind, index) in casts {
        let (mut world, unit) = blink_world(0xB12)?;
        let caster = world.spawn_unit(kind, PlayerId(0), point(11.0, 8.0))?;
        if let Some(entity) = world.entities.get_mut(&caster) {
            entity.energy = SimScalar::from_num(200);
        }
        // One tick first, so the caster's player sees its target (abilities may require it).
        world.step(&bundle(1));
        if !world.execute_ability(caster, index, None, Some(unit)) {
            continue;
        }
        let mut flown = false;
        for tick in 2..=120 {
            world.step(&bundle(tick));
            let in_flight = world
                .entities
                .get(&unit)
                .is_some_and(|entity| entity.displacement.is_some());
            flown |= in_flight;
            if flown && !in_flight {
                return Ok(Some((world, unit)));
            }
        }
    }
    Ok(None)
}

/// P-3: a `Displace` flight (a pull, 5.0.14's Abduct is a launch, not a `CEffectTeleport`) moves
/// a unit without a teleport.
#[test]
fn a_displacement_counts_no_teleport() -> TestResult {
    let (world, unit) = displaced_blinker()?.ok_or("a shipped ability displaces its target")?;
    let entity = world.entities.get(&unit).ok_or("the flown unit lives")?;
    assert_ne!(entity.position, point(8.0, 8.0), "the unit was flown");
    assert_eq!(entity.teleport_count, 0, "a displacement is no teleport");
    Ok(())
}

/// The shooter's position, in world units.
const SHOOTER: (f64, f64) = (10.0, 16.0);
/// The target's position, in world units: six from the shooter.
const TARGET: (f64, f64) = (16.0, 16.0);

/// A shipped mobile ground archetype whose ground weapon is a missile without an area.
fn missile_archetype(world: &SimWorld) -> Result<UnitArchetype, String> {
    world
        .catalog
        .archetypes()
        .filter(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
        .find(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .is_some_and(|w| w.projectile_speed.is_some() && w.splash.is_none())
        })
        .cloned()
        .ok_or_else(|| "the shipped catalog has a ground missile weapon".to_owned())
}

/// The bench: a flat 32x32 world, a shooter of player 0 at [`SHOOTER`] carrying a missile
/// weapon, and an unarmed blinker of player 1 at [`TARGET`].
struct Bench {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
    /// The blinker's archetype, for bystanders.
    target_type: UnitTypeId,
}

/// Builds the bench with the shipped missile weapon, its range widened to 20 so that nobody
/// walks, then changed by `edit`; every other field stays as the catalog resolved it.
fn bench(
    seed: u64,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Bench, Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let mut shooter_arch = missile_archetype(&world)?;
    let mut weapon = shooter_arch
        .weapon_against(MoverType::Ground, None)
        .cloned()
        .ok_or("invariant: the archetype was chosen for its ground weapon")?;
    weapon.max_range = SimScalar::from_num(20);
    weapon.min_range = SimScalar::ZERO;
    edit(&mut weapon);
    shooter_arch.unit_type_id = free_type(&world);
    "scratch_missile_shooter".clone_into(&mut shooter_arch.slug);
    shooter_arch.weapon = Some(weapon.clone());
    shooter_arch.ground_weapon = Some(weapon);
    shooter_arch.air_weapon = None;
    let shooter_type = shooter_arch.unit_type_id;
    world.catalog.register(shooter_arch);
    let target_type = register_blinker(&mut world)?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), point(SHOOTER.0, SHOOTER.1))?;
    let target = world.spawn_unit(target_type, PlayerId(1), point(TARGET.0, TARGET.1))?;
    Ok(Bench {
        world,
        shooter,
        target,
        target_type,
    })
}

impl Bench {
    /// Orders the attack and steps until the first missile is in flight; returns the tick.
    fn launch(&mut self) -> Result<u32, String> {
        self.world
            .entities
            .get_mut(&self.shooter)
            .ok_or("the shooter exists")?
            .issue_order(UnitOrder::attack(self.target), false)
            .map_err(|error| format!("{error:?}"))?;
        for tick in 1..=60 {
            self.world.step(&bundle(tick));
            if !self.world.projectiles.is_empty() {
                return Ok(tick);
            }
        }
        Err("the missile launched within 60 ticks".to_owned())
    }

    /// Blinks the target to `destination` between two ticks.
    fn blink(&mut self, destination: SimVec2) -> Result<(), String> {
        if self
            .world
            .execute_ability(self.target, 0, Some(destination), None)
        {
            Ok(())
        } else {
            Err("the blink is admitted".to_owned())
        }
    }

    /// Steps after `from` until the missile lands, checking on every tick that it flies on at
    /// its speed toward `point` with no target. Returns the landing tick.
    fn fly_to_point(&mut self, from: u32, point: SimVec2) -> Result<u32, String> {
        for tick in from + 1..from + 200 {
            let before = self
                .world
                .projectiles
                .first()
                .map(|p| (p.current_pos.distance(p.target_pos), p.speed));
            self.world.step(&bundle(tick));
            let Some((remaining, speed)) = before else {
                return Err(format!("the missile was gone before tick {tick}"));
            };
            match self.world.projectiles.first() {
                None if remaining <= speed => return Ok(tick),
                None => {
                    return Err(format!(
                        "tick {tick}: the missile vanished {remaining:?} short of its point"
                    ))
                }
                Some(missile) => {
                    if missile.target_id.is_some() {
                        return Err(format!("tick {tick}: the missile still has a target"));
                    }
                    if missile.target_pos != point {
                        return Err(format!(
                            "tick {tick}: the missile aims at {:?}, not the last point {point:?}",
                            missile.target_pos
                        ));
                    }
                    if remaining <= speed {
                        return Err(format!("tick {tick}: the missile did not land in reach"));
                    }
                }
            }
        }
        Err("the missile landed within 200 ticks".to_owned())
    }

    /// Steps after `from` until no missile is in flight; returns the last tick stepped.
    fn until_spent(&mut self, from: u32) -> Result<u32, String> {
        for tick in from + 1..from + 200 {
            self.world.step(&bundle(tick));
            if self.world.projectiles.is_empty() {
                return Ok(tick);
            }
        }
        Err("the missile was spent within 200 ticks".to_owned())
    }

    /// Life and shields of `id`, so that a strike absorbed by shields still counts.
    fn hp(&self, id: EntityId) -> Option<(SimScalar, SimScalar)> {
        self.world
            .entities
            .get(&id)
            .map(|entity| (entity.hp, entity.shields))
    }
}

/// Where the target blinks to: ten world units up, inside the shooter's widened range.
fn blink_destination() -> SimVec2 {
    point(TARGET.0, TARGET.1 + 10.0)
}

/// P-1: a missile in flight at a unit that Blinks flies on at its speed to the point where the
/// unit stood before the Blink, lands there `ceil(distance / speed)` ticks later, and strikes
/// nobody.
#[test]
fn a_missile_at_a_blinker_lands_where_it_stood() -> TestResult {
    let mut bench = bench(0xB20, |_| {})?;
    let full = bench.hp(bench.target).ok_or("the target exists")?;
    let launched = bench.launch()?;
    let stood = bench.world.entities[&bench.target].position;
    let (distance, speed) = bench
        .world
        .projectiles
        .first()
        .map(|p| (p.current_pos.distance(stood), p.speed))
        .ok_or("the missile is in flight")?;
    bench.blink(blink_destination())?;
    let landed = bench.fly_to_point(launched, stood)?;
    let flight = (distance / speed).saturating_ceil().saturating_to_u32();
    assert_eq!(landed, launched + flight, "the missile lands at its speed");
    assert_eq!(
        bench.hp(bench.target),
        Some(full),
        "the blinker is not struck"
    );
    Ok(())
}

/// A scratch area of three zones (1, 1.5 and 2 world units at 100, 50 and 25 %).
fn scratch_splash() -> SplashSpec {
    let zone = |radius: f64, fraction: f64| SplashZone {
        radius: SimScalar::from_num(radius),
        fraction: SimScalar::from_num(fraction),
    };
    SplashSpec {
        zones: vec![zone(1.0, 1.0), zone(1.5, 0.5), zone(2.0, 0.25)],
        shape: SplashShape::Circle,
        friendly_fire: false,
        offset_by_unit_radius: false,
        impact: SplashImpact::Target,
        target_radius_below: None,
        cliff: SplashCliff::Same,
        strike_target_on_other_level: false,
    }
}

/// P-1 with an area: a splash missile at a unit that Blinks strikes around the point where the
/// unit stood: a bystander of its side inside the area is struck, one beyond the last zone and
/// the blinker are not.
#[test]
fn a_splash_missile_at_a_blinker_strikes_the_old_point() -> TestResult {
    let mut bench = bench(0xB21, |weapon| weapon.splash = Some(scratch_splash()))?;
    let inside = bench.world.spawn_unit(
        bench.target_type,
        PlayerId(1),
        point(TARGET.0, TARGET.1 + 1.5),
    )?;
    let outside = bench.world.spawn_unit(
        bench.target_type,
        PlayerId(1),
        point(TARGET.0, TARGET.1 - 5.0),
    )?;
    let full = [bench.target, inside, outside].map(|id| bench.hp(id));
    let launched = bench.launch()?;
    bench.blink(blink_destination())?;
    bench.until_spent(launched)?;
    assert_eq!(bench.hp(bench.target), full[0], "the blinker is not struck");
    assert_ne!(
        bench.hp(inside),
        full[1],
        "the area strikes around the old point"
    );
    assert_eq!(
        bench.hp(outside),
        full[2],
        "nothing beyond the last zone is struck"
    );
    Ok(())
}

/// P-1: a missile launched after the Blink tracks the unit at its new position and strikes it.
#[test]
fn a_missile_launched_after_the_blink_strikes() -> TestResult {
    let mut bench = bench(0xB22, |_| {})?;
    let full = bench.hp(bench.target).ok_or("the target exists")?;
    bench.blink(point(TARGET.0, TARGET.1 + 4.0))?;
    let launched = bench.launch()?;
    assert!(
        bench
            .world
            .projectiles
            .first()
            .is_some_and(|p| p.target_id == Some(bench.target)),
        "the missile homes on the blinker"
    );
    bench.until_spent(launched)?;
    assert_ne!(bench.hp(bench.target), Some(full), "the blinker is struck");
    Ok(())
}

