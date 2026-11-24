//! A missile whose target is lost flies on to the target's last point and lands there (B-54).
//!
//! # Responsibility
//! Pins plan `2026-11-23-vision-and-detection` P-1 and P-2: every attack missile keeps
//! `StarCraft` II 5.0.14's `CEffectLaunchMissile` default `PointFallback 1` (`core`
//! EffectData:97). Once its target fails the weapon's filters in flight (it dies, is loaded,
//! turns invulnerable or cloaks undetected), the missile keeps flying at its speed to the last
//! point it tracked and lands there. Its strike reaches no unit; a splash weapon's area strikes
//! around the point. A target that becomes available again is not re-acquired.
//!
//! # How it fits
//! The weapons are scratch copies of a shipped missile weapon found by property (a ground
//! weapon with a projectile speed and no splash), with the range widened so that nobody walks;
//! their target-loss behaviour is left as the catalog resolved it, so these tests read the
//! shipped default. The fizzle and retarget variants are pinned in
//! `combat_retaliation_and_projectile_tests.rs`.
//!
//! # Invariants
//! - No unit is named: archetypes and weapons are found by their properties (charter §3.3).
//! - Every world is flat, so no cliff rule takes part.

use sim_core::{SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerId, ProjectileTargetLossBehavior, SimScalar, SimVec2, SplashCliff,
    SplashImpact, SplashShape, SplashZone, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The shooter's position, in world units.
const SHOOTER: (i32, i32) = (10, 16);
/// The target's position, in world units: six from the shooter.
const TARGET: (i32, i32) = (16, 16);

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

/// The bench: a flat world, a shooter carrying `weapon`, an unarmed target, and the target's
/// archetype (so bystanders can be spawned).
struct Bench {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
    dummy: UnitTypeId,
}

/// The shipped missile weapon, widened to reach across the bench; its target-loss behaviour
/// and every other field stay as the catalog resolved them.
fn shipped_missile(world: &SimWorld) -> Result<WeaponSpec, String> {
    let arch = missile_archetype(world)?;
    let mut weapon = arch
        .weapon_against(MoverType::Ground, None)
        .cloned()
        .ok_or("invariant: the archetype was chosen for its ground weapon")?;
    weapon.max_range = SimScalar::from_num(20);
    weapon.min_range = SimScalar::ZERO;
    Ok(weapon)
}

/// Builds the bench with the shipped missile weapon, changed by `edit` first.
fn bench(
    seed: u64,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Bench, Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let base = missile_archetype(&world)?;
    let mut weapon = shipped_missile(&world)?;
    edit(&mut weapon);
    let shooter_type = free_type(&world);
    let mut shooter_arch = base.clone();
    shooter_arch.unit_type_id = shooter_type;
    "scratch_missile_shooter".clone_into(&mut shooter_arch.slug);
    shooter_arch.weapon = Some(weapon.clone());
    shooter_arch.ground_weapon = Some(weapon);
    shooter_arch.air_weapon = None;
    world.catalog.register(shooter_arch);
    let dummy = UnitTypeId(shooter_type.0 + 1);
    let mut dummy_arch = base;
    dummy_arch.unit_type_id = dummy;
    "scratch_unarmed_target".clone_into(&mut dummy_arch.slug);
    dummy_arch.weapon = None;
    dummy_arch.ground_weapon = None;
    dummy_arch.air_weapon = None;
    world.catalog.register(dummy_arch);
    let shooter = world.spawn_unit(
        shooter_type,
        PlayerId(0),
        SimVec2::from_i32(SHOOTER.0, SHOOTER.1),
    )?;
    let target = world.spawn_unit(dummy, PlayerId(1), SimVec2::from_i32(TARGET.0, TARGET.1))?;
    Ok(Bench {
        world,
        shooter,
        target,
        dummy,
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

    /// Life and shields of `id`, so that a strike absorbed by shields still counts.
    fn hp(&self, id: EntityId) -> Option<(SimScalar, SimScalar)> {
        self.world
            .entities
            .get(&id)
            .map(|entity| (entity.hp, entity.shields))
    }
}

/// Plays one cause of loss: `lose` is applied to the bench right after the launch. Asserts the
/// flight to the last point and that the target took nothing.
fn loses_and_lands(seed: u64, lose: impl FnOnce(&mut Bench) -> TestResult) -> TestResult {
    let mut bench = bench(seed, |_| {})?;
    let full = bench.hp(bench.target).ok_or("the target exists")?;
    let launched = bench.launch()?;
    let last = bench.world.entities[&bench.target].position;
    lose(&mut bench)?;
    bench.fly_to_point(launched, last)?;
    if let Some(hp) = bench.hp(bench.target) {
        assert_eq!(hp, full, "a lost missile strikes no unit");
    }
    Ok(())
}

/// P-1: the target dies in flight. The missile flies on to where it stood and lands there.
#[test]
fn a_missile_whose_target_dies_lands_on_the_last_point() -> TestResult {
    loses_and_lands(0x54A, |bench| {
        bench
            .world
            .entities
            .get_mut(&bench.target)
            .ok_or("the target exists")?
            .hp = SimScalar::ZERO;
        Ok(())
    })
}

/// P-1: the target is loaded into a transport in flight.
#[test]
fn a_missile_whose_target_loads_lands_on_the_last_point() -> TestResult {
    loses_and_lands(0x54B, |bench| {
        let carrier = bench
            .world
            .spawn_unit(bench.dummy, PlayerId(1), point(28.0, 28.0))?;
        bench
            .world
            .try_load_unit_into_transport(bench.target, carrier)
            .map_err(|error| format!("{error:?}"))?;
        Ok(())
    })
}

/// P-1: the target turns invulnerable in flight.
#[test]
fn a_missile_whose_target_turns_invulnerable_lands_on_the_last_point() -> TestResult {
    loses_and_lands(0x54C, |bench| {
        bench
            .world
            .entities
            .get_mut(&bench.target)
            .ok_or("the target exists")?
            .is_invulnerable = true;
        Ok(())
    })
}

/// P-1: the target cloaks in flight and the shooter's player has no detection.
#[test]
fn a_missile_whose_target_cloaks_undetected_lands_on_the_last_point() -> TestResult {
    loses_and_lands(0x54D, |bench| {
        bench
            .world
            .entities
            .get_mut(&bench.target)
            .ok_or("the target exists")?
            .is_cloaked = true;
        Ok(())
    })
}

/// P-1, permanence: a target that cloaks and uncloaks again before the impact, standing on the
/// point, is not struck.
#[test]
fn a_target_found_again_is_not_struck() -> TestResult {
    let mut bench = bench(0x54E, |_| {})?;
    let full = bench.hp(bench.target).ok_or("the target exists")?;
    let launched = bench.launch()?;
    let last = bench.world.entities[&bench.target].position;
    if let Some(target) = bench.world.entities.get_mut(&bench.target) {
        target.is_cloaked = true;
    }
    bench.world.step(&bundle(launched + 1));
    assert!(
        bench
            .world
            .projectiles
            .first()
            .is_some_and(|p| p.target_id.is_none()),
        "the missile lost its target"
    );
    if let Some(target) = bench.world.entities.get_mut(&bench.target) {
        target.is_cloaked = false;
    }
    bench.fly_to_point(launched + 1, last)?;
    assert_eq!(
        bench.hp(bench.target),
        Some(full),
        "a found-again target is not struck"
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

/// Plays a splash missile whose target dies in flight, with one bystander of the target's side
/// inside the area around the last point and one outside it. Returns whether each was struck.
fn lost_splash(
    behavior: Option<ProjectileTargetLossBehavior>,
) -> Result<(bool, bool), Box<dyn std::error::Error>> {
    let seed = 0x54F;
    let mut bench = bench(seed, |weapon| {
        weapon.splash = Some(scratch_splash());
        if let Some(behavior) = behavior {
            weapon.target_loss_behavior = behavior;
        }
    })?;
    let inside = bench
        .world
        .spawn_unit(bench.dummy, PlayerId(1), point(16.0, 17.5))?;
    let outside = bench
        .world
        .spawn_unit(bench.dummy, PlayerId(1), point(16.0, 21.0))?;
    let (inside_hp, outside_hp) = (
        bench.hp(inside).ok_or("spawned")?,
        bench.hp(outside).ok_or("spawned")?,
    );
    let launched = bench.launch()?;
    if let Some(target) = bench.world.entities.get_mut(&bench.target) {
        target.hp = SimScalar::ZERO;
    }
    for tick in launched + 1..launched + 200 {
        bench.world.step(&bundle(tick));
        if bench.world.projectiles.is_empty() {
            break;
        }
    }
    assert!(bench.world.projectiles.is_empty(), "the missile is spent");
    Ok((
        bench.hp(inside) != Some(inside_hp),
        bench.hp(outside) != Some(outside_hp),
    ))
}

/// P-1 with an area: the area strikes around the last point by its zones.
#[test]
fn a_lost_splash_missile_strikes_around_the_point() -> TestResult {
    let (inside, outside) = lost_splash(None)?;
    assert!(
        inside,
        "a unit inside the area around the last point is struck"
    );
    assert!(!outside, "a unit beyond the last zone is not struck");
    Ok(())
}

/// P-1 with an area, the variant that clears `PointFallback`: `Fizzle` strikes nobody.
#[test]
fn a_fizzling_splash_missile_strikes_nobody() -> TestResult {
    let (inside, outside) = lost_splash(Some(ProjectileTargetLossBehavior::Fizzle))?;
    assert!(!inside && !outside, "a fizzled missile lands nowhere");
    Ok(())
}

/// P-2: a missile whose target stays available lands on it on the same tick and for the same
/// damage whether the weapon falls back to the point or fizzles on a loss.
#[test]
fn a_tracked_missile_lands_as_before() -> TestResult {
    let mut outcomes = Vec::new();
    for behavior in [
        ProjectileTargetLossBehavior::ImpactLastKnownPosition,
        ProjectileTargetLossBehavior::Fizzle,
    ] {
        let mut bench = bench(0x550, |weapon| weapon.target_loss_behavior = behavior)?;
        let full = bench.hp(bench.target).ok_or("spawned")?;
        let launched = bench.launch()?;
        let mut landed = None;
        for tick in launched + 1..launched + 200 {
            bench.world.step(&bundle(tick));
            if bench.hp(bench.target) != Some(full) {
                landed = Some(tick);
                break;
            }
        }
        outcomes.push((
            landed.ok_or("the missile struck its target")?,
            bench.hp(bench.target),
        ));
    }
    assert_eq!(
        outcomes[0], outcomes[1],
        "tracking is unchanged by the loss rule"
    );
    Ok(())
}

/// D-1: the shipped default of every attack missile is the point fallback.
#[test]
fn every_shipped_missile_falls_back_to_the_point() -> TestResult {
    let world = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    for arch in world.catalog.archetypes() {
        for weapon in [&arch.weapon, &arch.ground_weapon, &arch.air_weapon]
            .into_iter()
            .flatten()
            .filter(|weapon| weapon.projectile_speed.is_some())
        {
            assert_eq!(
                weapon.target_loss_behavior,
                ProjectileTargetLossBehavior::ImpactLastKnownPosition,
                "{}'s missile keeps 5.0.14's PointFallback",
                weapon.name
            );
        }
    }
    Ok(())
}
