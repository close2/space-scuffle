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
//! - D-4, B-194 (P-4): a missile whose weapon clears `missile_validation.weapon` (5.0.14's
//!   Brood Lord missiles, `ValidateWeapon 0`, `liberty` EffectData:2668, :2749, :2780) follows
//!   its target while it lives and is not loaded, wherever it goes, and strikes it only if the
//!   target passes the weapon's filters on the landing tick (`BroodlingEscortFilters`, :2679,
//!   :2785).
//! - D-3, D-5, B-185 (P-6 to P-10): a weapon starts, lands and tracks only a target its
//!   player sees (5.0.14's `TargetFilters Visible`, `core` WeaponData:29): its cell `Visible` to
//!   the attacker's player, or the target attack-revealed to that player (B-55). A missile whose
//!   target walks into fog lands on the point (`ValidateWeapon`). Acquisition drops an automatic
//!   target lost to fog for the best seen candidate, or resumes the order below it; it releases
//!   a hold or tracking target, and ends an immobile unit's attack order.
//! - Amendment 2, B-195 (Q0106 part 1 (a)): a mobile unit's attack order on a target that leaves
//!   its player's vision walks to where the target was last seen and ends there, and resumes if
//!   the target is seen before; "seen" is the cell at the target's centre (Q0106 part 4 (a)). A
//!   target never seen under the order is followed and held fire on, as before.
//! - Plan `2026-11-25-wind-up-and-hold-facing` D-1 (A-3, A-5; B-207): a wind-up whose target
//!   fails the same availability test before a tick starts (it boarded, cloaked unrevealed, or
//!   left its player's sight) has ended before that tick's movement phase and holds nothing; a
//!   lost swing charges no cooldown and draws no random delay.
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
//! - Every world is flat unless a test says otherwise. Sight is whole cells: a cell is seen
//!   when its offset from the viewer's cell lies within the sight radius, so the vision tests
//!   place units at cell centres and count offsets in cells.

use sim_core::catalog::{AcquireLevel, ContactRules, MovementPipeline};
use sim_core::{LastSeen, SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
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

/// Sets `missile_validation.weapon` to `false`: the missile validates the target at impact.
fn validates_at_impact(weapon: &mut WeaponSpec) {
    weapon.missile_validation.weapon = false;
}

/// A shipped mobile archetype that detects.
fn mobile_detector(world: &SimWorld) -> Option<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .find(|arch| arch.detection_radius.is_some() && arch.max_speed > SimScalar::ZERO)
        .map(|arch| arch.unit_type_id)
}

/// Cloaks the target right after the launch and gives it a move order up the map, then steps
/// until the missile is spent, checking on every tick it is in flight that it still homes on the
/// target's current position. With `detector`, a mobile detector of the shooter's player is
/// placed beside the target's path after one tick of flight, before the landing. Returns whether
/// the target was struck.
fn cloaked_flight(seed: u64, detector: bool) -> Result<bool, Box<dyn std::error::Error>> {
    let mut bench = bench(seed, validates_at_impact)?;
    let full = bench.hp(bench.target).ok_or("the target exists")?;
    let launched = bench.launch()?;
    let target = bench
        .world
        .entities
        .get_mut(&bench.target)
        .ok_or("the target exists")?;
    target.is_cloaked = true;
    target
        .issue_order(
            UnitOrder::Move {
                target: point(TARGET.0, TARGET.1 + 8.0),
            },
            false,
        )
        .map_err(|error| format!("{error:?}"))?;
    for tick in launched + 1..launched + 200 {
        if detector && tick == launched + 2 {
            // After one tick of undetected flight, which a missile validating the weapon would
            // not survive.
            let kind = mobile_detector(&bench.world).ok_or("a mobile detector is shipped")?;
            bench
                .world
                .spawn_unit(kind, PlayerId(0), point(TARGET.0 - 2.0, TARGET.1 + 1.0))?;
        }
        bench.world.step(&bundle(tick));
        let Some(missile) = bench.world.projectiles.first() else {
            return Ok(bench.hp(bench.target) != Some(full));
        };
        assert_eq!(
            missile.target_id,
            Some(bench.target),
            "tick {tick}: the missile keeps its cloaked target"
        );
        assert_eq!(
            missile.target_pos, bench.world.entities[&bench.target].position,
            "tick {tick}: the missile homes on the target's current position"
        );
    }
    Err("the missile was spent within 200 ticks".into())
}

/// P-4: a missile that skips the weapon check follows a target that cloaks undetected and
/// lands on it; undetected at the landing, the target is not struck.
#[test]
fn a_missile_that_skips_the_weapon_check_follows_a_cloaking_target() -> TestResult {
    assert!(
        !cloaked_flight(0xB30, false)?,
        "an undetected target fails the filter at impact"
    );
    Ok(())
}

/// P-4: the same missile strikes its cloaked target when a detector of the shooter's player
/// reveals it before the landing: the filter is checked at impact, not in flight.
#[test]
fn a_missile_that_skips_the_weapon_check_strikes_only_if_the_target_passes_at_impact() -> TestResult
{
    assert!(
        cloaked_flight(0xB31, true)?,
        "a target detected at the landing is struck"
    );
    Ok(())
}

/// P-4: a missile that skips the weapon check still follows only a living target: one that
/// dies in flight leaves it to land on its last point, striking nobody.
#[test]
fn a_missile_that_skips_the_weapon_check_lands_on_the_point_of_a_target_that_died() -> TestResult {
    let mut bench = bench(0xB32, validates_at_impact)?;
    let launched = bench.launch()?;
    let last = bench.world.entities[&bench.target].position;
    bench
        .world
        .entities
        .get_mut(&bench.target)
        .ok_or("the target exists")?
        .hp = SimScalar::ZERO;
    bench.fly_to_point(launched, last)?;
    Ok(())
}

/// Side of the vision arena, in cells.
const ARENA: u32 = 48;
/// The vision shooter's cell; units stand at cell centres.
const SHOOTER_CELL: (u32, u32) = (10, 24);

/// The centre of cell `(x, y)` in world units.
fn cell_centre(x: u32, y: u32) -> SimVec2 {
    point(f64::from(x) + 0.5, f64::from(y) + 0.5)
}

/// A shipped mobile ground archetype that acquires targets on its own (not a worker's
/// defensive level), whose ground weapon is ranged and strikes at once (no missile, no area)
/// and can hit the archetype itself, with that weapon.
fn instant_archetype(world: &SimWorld) -> Result<(UnitArchetype, WeaponSpec), String> {
    world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && arch.acquire_level != AcquireLevel::Defensive
        })
        .find_map(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .filter(|w| {
                    w.projectile_speed.is_none()
                        && !w.melee
                        && w.splash.is_none()
                        && w.target_flags.intersects(arch.attributes)
                })
                .map(|w| (arch.clone(), w.clone()))
        })
        .ok_or_else(|| "the shipped catalog has an instant ground weapon".to_owned())
}

/// The centre of the cell `columns` to the right of the shooter's, in its row.
fn ahead(columns: u32) -> SimVec2 {
    cell_centre(SHOOTER_CELL.0 + columns, SHOOTER_CELL.1)
}

/// The vision bench: a flat arena, a shooter of player 0 at [`SHOOTER_CELL`] carrying a weapon
/// whose reach is widened beyond its sight, and unarmed blinkers of player 1 to aim at.
struct Arena {
    world: SimWorld,
    shooter: EntityId,
    /// The shooter's sight in whole cells, rounded down: a cell this many columns away in the
    /// shooter's row is seen, one more is not.
    sight_cells: u32,
    /// The blinker archetype, for targets and spotters.
    dummy: UnitTypeId,
}

/// Builds the arena with `pick`'s archetype and weapon for the shooter, the weapon's range set
/// to the shooter's sight plus 6 and its minimum range cleared, then changed by `edit`.
fn arena(
    seed: u64,
    pick: impl Fn(&SimWorld) -> Result<(UnitArchetype, WeaponSpec), String>,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<Arena, Box<dyn std::error::Error>> {
    let map = GameMap::new(ARENA, ARENA, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let (mut arch, mut weapon) = pick(&world)?;
    weapon.max_range = arch.sight_radius + SimScalar::from_i32(6);
    weapon.min_range = SimScalar::ZERO;
    edit(&mut weapon);
    arch.unit_type_id = free_type(&world);
    "scratch_vision_shooter".clone_into(&mut arch.slug);
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon);
    arch.air_weapon = None;
    arch.detection_radius = None;
    let sight_cells = arch.sight_radius.saturating_to_u32();
    let kind = arch.unit_type_id;
    world.catalog.register(arch);
    let dummy = register_blinker(&mut world)?;
    let shooter = world.spawn_unit(
        kind,
        PlayerId(0),
        cell_centre(SHOOTER_CELL.0, SHOOTER_CELL.1),
    )?;
    Ok(Arena {
        world,
        shooter,
        sight_cells,
        dummy,
    })
}

impl Arena {
    /// A cell the shooter sees, two columns inside its sight.
    fn seen(&self) -> SimVec2 {
        ahead(self.sight_cells - 2)
    }

    /// A cell beyond the shooter's sight and inside its weapon's reach.
    fn unseen(&self) -> SimVec2 {
        ahead(self.sight_cells + 2)
    }

    /// Spawns a blinker of `owner` at `at`.
    fn dummy_at(&mut self, owner: u8, at: SimVec2) -> Result<EntityId, Box<dyn std::error::Error>> {
        Ok(self.world.spawn_unit(self.dummy, PlayerId(owner), at)?)
    }

    /// Gives `id` the order `order`, replacing its queue.
    fn order(&mut self, id: EntityId, order: UnitOrder) -> Result<(), String> {
        self.world
            .entities
            .get_mut(&id)
            .ok_or("the unit exists")?
            .issue_order(order, false)
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }

    /// Blinks `id` (a blinker) to `to`.
    fn blink(&mut self, id: EntityId, to: SimVec2) -> Result<(), String> {
        if self.world.execute_ability(id, 0, Some(to), None) {
            Ok(())
        } else {
            Err("the blink is admitted".to_owned())
        }
    }

    /// Life and shields of `id`.
    fn hp(&self, id: EntityId) -> Option<(SimScalar, SimScalar)> {
        self.world.entities.get(&id).map(|e| (e.hp, e.shields))
    }

    /// Steps ticks `first..=last`, failing if the shooter starts a swing on any of them.
    fn no_swing(&mut self, first: u32, last: u32) -> Result<(), String> {
        for tick in first..=last {
            self.world.step(&bundle(tick));
            if matches!(
                self.world
                    .entities
                    .get(&self.shooter)
                    .map(|e| &e.weapon_phase),
                Some(sim_core::WeaponPhase::Windup { .. })
            ) {
                return Err(format!("tick {tick}: the shooter swings"));
            }
        }
        Ok(())
    }

    /// Steps from `from` until `id` takes damage; returns that tick.
    fn until_struck(&mut self, id: EntityId, from: u32) -> Result<u32, String> {
        let full = self.hp(id);
        for tick in from..from + 150 {
            self.world.step(&bundle(tick));
            if self.hp(id) != full {
                return Ok(tick);
            }
        }
        Err(format!("{id:?} was struck within 150 ticks"))
    }

    /// The shooter's front order.
    fn front(&self) -> Option<UnitOrder> {
        self.world
            .entities
            .get(&self.shooter)
            .and_then(|e| e.order_queue.front().cloned())
    }
}

/// P-6: a unit ordered to attack an enemy standing in its weapon's reach but in a cell its
/// player does not see starts no swing; the order is kept (B-195) and the enemy is not struck.
#[test]
fn no_swing_starts_on_an_unseen_target() -> TestResult {
    let mut arena = arena(0xB50, instant_archetype, |_| {})?;
    let unseen = arena.unseen();
    let target = arena.dummy_at(1, unseen)?;
    let full = arena.hp(target);
    arena.order(arena.shooter, UnitOrder::attack(target))?;
    arena.no_swing(1, 100)?;
    assert_eq!(arena.hp(target), full, "the unseen target is not struck");
    assert_eq!(
        arena.front(),
        Some(UnitOrder::attack(target)),
        "the player's order is kept"
    );
    Ok(())
}

/// P-6: a strike that falls due after the release lands only on a target its player still
/// sees: the target of a two-strike burst blinks out of sight after the first strike, and the
/// second is lost.
#[test]
fn a_pending_strike_holds_on_an_unseen_target() -> TestResult {
    let mut arena = arena(0xB51, instant_archetype, |weapon| {
        weapon.attacks_count = 2;
        weapon.first_strike_delay_ticks = 0;
        weapon.strike_interval_ticks = 10;
        weapon.followup_max_distance = None;
        weapon.period_ticks = weapon.period_ticks.max(80);
    })?;
    let seen = arena.seen();
    let target = arena.dummy_at(1, seen)?;
    arena.order(arena.shooter, UnitOrder::attack(target))?;
    let first = arena.until_struck(target, 1)?;
    let after_first = arena.hp(target);
    let unseen = arena.unseen();
    arena.blink(target, unseen)?;
    steps(&mut arena.world, first + 1, first + 30);
    assert_eq!(
        arena.hp(target),
        after_first,
        "the second strike is lost on the unseen target"
    );
    Ok(())
}

/// P-6: once a unit of the same player sees the target's cell, the held attack lands.
#[test]
fn a_spotter_lets_the_strike_land() -> TestResult {
    let mut arena = arena(0xB52, instant_archetype, |_| {})?;
    let unseen = arena.unseen();
    let target = arena.dummy_at(1, unseen)?;
    arena.order(arena.shooter, UnitOrder::attack(target))?;
    arena.no_swing(1, 40)?;
    let spotter_at = ahead(arena.sight_cells + 4);
    arena.dummy_at(0, spotter_at)?;
    arena.until_struck(target, 41)?;
    Ok(())
}

/// P-6: a target revealed to the attacker's player by its own attacks (B-55) is struck though
/// its cell is not seen.
#[test]
fn an_attack_revealed_target_is_struck() -> TestResult {
    let mut arena = arena(0xB53, instant_archetype, |_| {})?;
    let unseen = arena.unseen();
    let target = arena.dummy_at(1, unseen)?;
    arena.order(arena.shooter, UnitOrder::attack(target))?;
    arena.no_swing(1, 40)?;
    arena
        .world
        .entities
        .get_mut(&target)
        .ok_or("the target exists")?
        .attack_reveal_ticks[sim_types::PlayerSlot::ALL[0]] = 54;
    arena.until_struck(target, 41)?;
    Ok(())
}

/// The missile bench of [`arena`]: the shooter carries the shipped ground missile.
fn missile_pick(world: &SimWorld) -> Result<(UnitArchetype, WeaponSpec), String> {
    let arch = missile_archetype(world)?;
    let weapon = arch
        .weapon_against(MoverType::Ground, None)
        .cloned()
        .ok_or("invariant: the archetype was chosen for its ground missile")?;
    Ok((arch, weapon))
}

/// A missile flight into fog: the arena, the target, the launch tick and the target's life and
/// shields before the launch.
type FogFlight = (Arena, EntityId, u32, Option<(SimScalar, SimScalar)>);

/// Launches a slow missile (0.1 world units a tick) at a target standing at the last seen
/// column of the shooter's row, puts the shooter on hold, and sends the target walking two
/// columns away, out of the shooter's sight, while the missile flies. Returns
/// the arena, the target, the launch tick and the target's life before the launch.
fn missile_into_fog(
    seed: u64,
    edit: impl FnOnce(&mut WeaponSpec),
) -> Result<FogFlight, Box<dyn std::error::Error>> {
    // A slow missile, so the walk out of sight surely comes before the landing.
    let mut arena = arena(seed, missile_pick, |weapon| {
        weapon.projectile_speed = Some(SimScalar::from_num(0.1));
        weapon.max_projectile_lifetime_ticks = 400;
        edit(weapon);
    })?;
    let edge = ahead(arena.sight_cells);
    let target = arena.dummy_at(1, edge)?;
    let full = arena.hp(target);
    arena.order(arena.shooter, UnitOrder::attack(target))?;
    let mut launched = None;
    for tick in 1..=60 {
        arena.world.step(&bundle(tick));
        if !arena.world.projectiles.is_empty() {
            launched = Some(tick);
            break;
        }
    }
    let launched = launched.ok_or("the missile launched within 60 ticks")?;
    // The shooter holds its ground, so only the missile's rule is measured: under its attack
    // order it would walk to where the target was last seen (Amendment 2), see it again and fire
    // anew.
    arena.order(arena.shooter, UnitOrder::HoldPosition)?;
    let beyond = ahead(arena.sight_cells + 2);
    arena.order(target, UnitOrder::Move { target: beyond })?;
    Ok((arena, target, launched, full))
}

/// P-7: a missile whose target walks into a cell its player does not see loses it and lands on
/// the last point it tracked, striking nobody.
#[test]
fn a_missile_whose_target_walks_into_fog_lands_on_the_point() -> TestResult {
    let (mut arena, target, launched, full) = missile_into_fog(0xB55, |_| {})?;
    let mut last = None;
    let mut lost_at = None;
    for tick in launched + 1..launched + 400 {
        arena.world.step(&bundle(tick));
        let Some(missile) = arena.world.projectiles.first() else {
            break;
        };
        match (missile.target_id, lost_at) {
            (Some(_), None) => last = Some(missile.target_pos),
            (None, None) => lost_at = Some(tick),
            (None, Some(_)) => {}
            (Some(_), Some(at)) => {
                return Err(format!("the missile found its target again after {at}").into())
            }
        }
        if lost_at.is_some() {
            assert_eq!(
                Some(missile.target_pos),
                last,
                "tick {tick}: it flies to the last point"
            );
        }
    }
    assert!(lost_at.is_some(), "the missile lost its target in the fog");
    assert!(arena.world.projectiles.is_empty(), "the missile landed");
    assert_eq!(arena.hp(target), full, "the target is not struck");
    Ok(())
}

/// P-4: a missile that skips the weapon check follows its target into the fog and lands on it,
/// and strikes it only if its player sees it at the landing: here it does not.
#[test]
fn a_missile_that_skips_the_weapon_check_follows_a_target_into_fog() -> TestResult {
    let (mut arena, target, launched, full) = missile_into_fog(0xB56, validates_at_impact)?;
    for tick in launched + 1..launched + 400 {
        arena.world.step(&bundle(tick));
        let Some(missile) = arena.world.projectiles.first() else {
            break;
        };
        assert_eq!(
            missile.target_id,
            Some(target),
            "tick {tick}: it keeps its target"
        );
    }
    assert!(arena.world.projectiles.is_empty(), "the missile landed");
    let position = arena.world.entities[&target].position;
    assert!(
        position.x > ahead(arena.sight_cells).x + SimScalar::HALF,
        "premise: the target stands beyond the shooter's sight at the landing"
    );
    assert_eq!(
        arena.hp(target),
        full,
        "an unseen target fails the filter at impact"
    );
    Ok(())
}

/// Sends the shooter attack-moving away from `target`'s side, steps until it has acquired the
/// seen `target` (an automatic attack), then blinks `target` out of sight, inside reach.
/// Returns the tick after the blink.
fn acquire_then_lose(
    arena: &mut Arena,
    target: EntityId,
) -> Result<u32, Box<dyn std::error::Error>> {
    arena.order(
        arena.shooter,
        UnitOrder::AttackMove {
            target: cell_centre(1, SHOOTER_CELL.1),
        },
    )?;
    let mut acquired = None;
    for tick in 1..=20 {
        arena.world.step(&bundle(tick));
        if matches!(
            arena.front(),
            Some(UnitOrder::AttackTarget { target: t, origin: sim_types::AttackOrigin::Acquired }) if t == target
        ) {
            acquired = Some(tick);
            break;
        }
    }
    let acquired = acquired.ok_or("the shooter acquired the seen target")?;
    let at = arena.world.entities[&arena.shooter].position;
    let beyond = at
        + SimVec2::new(
            SimScalar::from_num(f64::from(arena.sight_cells) + 2.0),
            SimScalar::ZERO,
        );
    arena.blink(target, beyond)?;
    Ok(acquired + 1)
}

/// P-8: an automatic attack whose target is lost to fog gives way to the best seen candidate.
#[test]
fn an_acquired_target_lost_to_fog_gives_way_to_a_seen_one() -> TestResult {
    let mut arena = arena(0xB57, instant_archetype, |_| {})?;
    let seen = arena.seen();
    let lost = arena.dummy_at(1, seen)?;
    let next = acquire_then_lose(&mut arena, lost)?;
    let near = arena.world.entities[&arena.shooter].position;
    let other = arena.dummy_at(1, near + SimVec2::from_i32(2, 2))?;
    steps(&mut arena.world, next, next + 1);
    assert!(
        matches!(arena.front(), Some(UnitOrder::AttackTarget { target, .. }) if target == other),
        "the seen candidate replaces the lost target, front {:?}",
        arena.front()
    );
    Ok(())
}

/// P-8: with no other candidate, the automatic attack is dropped and the attack-move below it
/// resumes; the lost target is not struck again.
#[test]
fn an_acquired_target_lost_to_fog_with_none_the_attack_move_resumes() -> TestResult {
    let mut arena = arena(0xB58, instant_archetype, |_| {})?;
    let seen = arena.seen();
    let lost = arena.dummy_at(1, seen)?;
    let next = acquire_then_lose(&mut arena, lost)?;
    let after = arena.hp(lost);
    steps(&mut arena.world, next, next + 1);
    assert!(
        matches!(arena.front(), Some(UnitOrder::AttackMove { .. })),
        "the attack-move resumes, front {:?}",
        arena.front()
    );
    steps(&mut arena.world, next + 2, next + 80);
    assert_eq!(arena.hp(lost), after, "the lost target is not struck");
    Ok(())
}

/// P-8: a seen target out of weapon reach is still chased by the automatic attack.
#[test]
fn a_seen_target_out_of_reach_is_still_chased() -> TestResult {
    let mut arena = arena(0xB59, instant_archetype, |weapon| {
        weapon.max_range = SimScalar::ONE;
    })?;
    let near = ahead(4);
    let target = arena.dummy_at(1, near)?;
    arena.order(
        arena.shooter,
        UnitOrder::AttackMove {
            target: cell_centre(1, SHOOTER_CELL.1),
        },
    )?;
    let start = arena.world.entities[&arena.shooter].position;
    let mut chased = false;
    for tick in 1..=60 {
        arena.world.step(&bundle(tick));
        let acquired = matches!(
            arena.front(),
            Some(UnitOrder::AttackTarget { target: t, .. }) if t == target
        );
        if acquired && arena.world.entities[&arena.shooter].position.x > start.x + SimScalar::ONE {
            chased = true;
            break;
        }
    }
    assert!(
        chased,
        "the shooter walks toward the seen target out of its reach"
    );
    Ok(())
}

/// P-8: a holding unit's target lost to fog is released and not struck.
#[test]
fn a_hold_target_lost_to_fog_is_released() -> TestResult {
    let mut arena = arena(0xB5A, instant_archetype, |_| {})?;
    let seen = arena.seen();
    let target = arena.dummy_at(1, seen)?;
    arena.order(arena.shooter, UnitOrder::HoldPosition)?;
    let struck = arena.until_struck(target, 1)?;
    assert_eq!(
        arena.world.entities[&arena.shooter].current_target,
        Some(target)
    );
    let unseen = arena.unseen();
    arena.blink(target, unseen)?;
    let after = arena.hp(target);
    steps(&mut arena.world, struck + 1, struck + 2);
    assert_eq!(
        arena.world.entities[&arena.shooter].current_target, None,
        "the holder releases the lost target"
    );
    steps(&mut arena.world, struck + 3, struck + 80);
    assert_eq!(arena.hp(target), after, "the lost target is not struck");
    Ok(())
}

/// P-8: an immobile unit's attack order on a target lost to fog ends.
#[test]
fn an_immobile_units_order_on_a_lost_target_ends() -> TestResult {
    let mut arena = arena(0xB5B, instant_archetype, |_| {})?;
    arena
        .world
        .entities
        .get_mut(&arena.shooter)
        .ok_or("the shooter exists")?
        .is_immobile = true;
    let seen = arena.seen();
    let target = arena.dummy_at(1, seen)?;
    arena.order(arena.shooter, UnitOrder::attack(target))?;
    let struck = arena.until_struck(target, 1)?;
    let unseen = arena.unseen();
    arena.blink(target, unseen)?;
    steps(&mut arena.world, struck + 1, struck + 2);
    assert!(
        !matches!(arena.front(), Some(UnitOrder::AttackTarget { .. })),
        "the immobile unit's order ends, front {:?}",
        arena.front()
    );
    Ok(())
}

/// Plan Amendment 2 (B-195, Q0106 part 1 (a)): the arena of [`arena`] on `pipeline`, with a
/// target of player 1 on hold (so a strike does not make it flee) at the seen cell [`Arena::seen`],
/// attacked by the shooter on a player's order until struck, then blinked to `to`. Returns the
/// arena, the target, the point the target was last seen at, and the tick after the strike.
fn lose_after_strike(
    seed: u64,
    pipeline: MovementPipeline,
    radius: Option<SimScalar>,
    to: impl FnOnce(&Arena) -> SimVec2,
) -> Result<(Arena, EntityId, SimVec2, u32), Box<dyn std::error::Error>> {
    let mut arena = arena(seed, instant_archetype, |_| {})?;
    on_pipeline(&mut arena.world, pipeline)?;
    if let Some(radius) = radius {
        let dummy = arena
            .world
            .catalog
            .get_mut(arena.dummy)
            .map_err(|e| format!("{e:?}"))?;
        dummy.collision_radius = radius;
    }
    let seen = arena.seen();
    let target = arena.dummy_at(1, seen)?;
    arena.order(target, UnitOrder::HoldPosition)?;
    arena.order(arena.shooter, UnitOrder::attack(target))?;
    let struck = arena.until_struck(target, 1)?;
    let destination = to(&arena);
    arena.blink(target, destination)?;
    Ok((arena, target, seen, struck + 1))
}

/// Switches `world` to the movement pipeline `pipeline`.
fn on_pipeline(world: &mut SimWorld, pipeline: MovementPipeline) -> Result<(), String> {
    let rules = world.catalog.movement_rules();
    let rules = rules
        .with_contact(ContactRules {
            pipeline,
            ..rules.contact()
        })
        .map_err(|e| format!("{e:?}"))?;
    world.catalog.set_movement_rules(rules);
    Ok(())
}

/// A cell the shooter never sees from its row: two columns inside its sight along the row and
/// beyond its sight across it, so the walk to the last seen point (along the row) and a chase by
/// the true position (across it) part at once.
fn across(arena: &Arena) -> SimVec2 {
    cell_centre(
        SHOOTER_CELL.0 + arena.sight_cells - 2,
        SHOOTER_CELL.1 + arena.sight_cells + 6,
    )
}

/// Plan Amendment 2 (B-195, Q0106 part 1 (a)), on both movement pipelines: a player's attack
/// order on a target that leaves its player's vision walks to where the target was last seen,
/// stops there and ends; the shooter keeps to its row (it does not follow the target's true
/// position) and strikes nothing on the way.
#[test]
fn a_players_order_on_a_lost_target_walks_to_where_it_was_last_seen_and_ends() -> TestResult {
    for pipeline in [MovementPipeline::Solver, MovementPipeline::Legacy] {
        let (mut arena, target, last_seen, from) =
            lose_after_strike(0xB5C, pipeline, None, across)?;
        let row = arena.world.entities[&arena.shooter].position.y;
        let after = arena.hp(target);
        let mut ended = None;
        for tick in from..from + 300 {
            arena.world.step(&bundle(tick));
            let at = arena.world.entities[&arena.shooter].position;
            assert!(
                (at.y - row).abs() < SimScalar::HALF,
                "{pipeline:?}, tick {tick}: the shooter keeps to its row, at {at:?}"
            );
            assert_eq!(
                arena.hp(target),
                after,
                "{pipeline:?}, tick {tick}: no strike"
            );
            if !matches!(arena.front(), Some(UnitOrder::AttackTarget { .. })) {
                ended = Some(tick);
                break;
            }
        }
        let ended = ended.ok_or_else(|| format!("{pipeline:?}: the order ends"))?;
        let at = arena.world.entities[&arena.shooter].position;
        assert!(
            at.distance(last_seen) <= SimScalar::HALF,
            "{pipeline:?}: the shooter stops where the target was last seen, {at:?} against {last_seen:?}"
        );
        assert!(
            matches!(arena.front(), None | Some(UnitOrder::Idle)),
            "{pipeline:?}: nothing follows the ended order, front {:?}",
            arena.front()
        );
        assert!(
            ended > from + 10,
            "{pipeline:?}: the shooter walked to the point, ending at tick {ended} (from {from})"
        );
    }
    Ok(())
}

/// Plan Amendment 2 (B-195, Q0106 part 1 (a)), on both movement pipelines: a target lost to fog
/// and seen again before the shooter reaches its last seen point is attacked again under the
/// same order. Until a spotter of the shooter's player sees it, the shooter walks along its row
/// toward the point.
#[test]
fn a_lost_target_seen_again_resumes_the_attack() -> TestResult {
    for pipeline in [MovementPipeline::Solver, MovementPipeline::Legacy] {
        let (mut arena, target, _, from) = lose_after_strike(0xB5D, pipeline, None, across)?;
        let start = arena.world.entities[&arena.shooter].position;
        steps(&mut arena.world, from, from + 9);
        let walked = arena.world.entities[&arena.shooter].position;
        assert!(
            walked.x > start.x && (walked.y - start.y).abs() < SimScalar::HALF,
            "{pipeline:?}: the shooter walks along its row to the point, {start:?} to {walked:?}"
        );
        assert_eq!(
            arena.front(),
            Some(UnitOrder::attack(target)),
            "{pipeline:?}: the order lasts while the shooter walks"
        );
        let hidden_at = arena.world.entities[&target].position;
        arena.dummy_at(0, hidden_at - SimVec2::from_i32(0, 2))?;
        let mut struck = false;
        let full = arena.hp(target);
        for tick in from + 10..from + 300 {
            arena.world.step(&bundle(tick));
            if arena.hp(target) != full {
                struck = true;
                break;
            }
            assert_eq!(
                arena.front(),
                Some(UnitOrder::attack(target)),
                "{pipeline:?}, tick {tick}: the order is kept once the target is seen"
            );
        }
        assert!(struck, "{pipeline:?}: the target seen again is struck");
    }
    Ok(())
}

/// Plan Amendment 2 (Q0106 part 4 (a)): a unit is seen when the cell at its centre is seen. A
/// large target whose body reaches into the shooter's sight while the cell at its centre lies one
/// column beyond is lost (the order walks to the last seen point); one column nearer, its centre
/// cell seen, it is kept and struck again.
#[test]
fn seen_means_the_cell_at_the_centre() -> TestResult {
    let radius = Some(SimScalar::ONE);
    let (mut arena, target, last_seen, from) =
        lose_after_strike(0xB5E, MovementPipeline::Solver, radius, |arena| {
            ahead(arena.sight_cells + 1)
        })?;
    arena.world.step(&bundle(from));
    let memory = arena.world.entities[&arena.shooter].movement.last_seen;
    assert_eq!(
        memory,
        Some(LastSeen {
            target,
            point: last_seen,
            lost: true
        }),
        "the target whose centre cell is unseen is lost"
    );
    let (mut arena, target, _, from) =
        lose_after_strike(0xB5E, MovementPipeline::Solver, radius, |arena| {
            ahead(arena.sight_cells)
        })?;
    arena.world.step(&bundle(from));
    let memory = arena.world.entities[&arena.shooter].movement.last_seen;
    assert_eq!(
        memory.map(|seen| (seen.target, seen.lost)),
        Some((target, false)),
        "the target whose centre cell is seen is kept"
    );
    arena.until_struck(target, from + 1)?;
    Ok(())
}

/// The plateau's first column: every cell with x at or above it is one cliff level up.
const PLATEAU: u32 = 16;

/// A 32x32 map whose cells with x >= [`PLATEAU`] stand one cliff level up, with no ramp.
fn plateau_map() -> Result<GameMap, sim_map::MapError> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..32 {
        for x in PLATEAU..32 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    Ok(map)
}

/// Registers a copy of `arch` armed with `weapon` alone under the next free identity.
fn register_armed(world: &mut SimWorld, arch: &UnitArchetype, weapon: WeaponSpec) -> UnitTypeId {
    let mut copy = arch.clone();
    copy.unit_type_id = free_type(world);
    format!("scratch_armed_{}", copy.unit_type_id.0).clone_into(&mut copy.slug);
    copy.air_weapon = arch.air_weapon.as_ref().map(|_| weapon.clone());
    copy.ground_weapon = Some(weapon.clone());
    copy.weapon = Some(weapon);
    copy.detection_radius = None;
    let id = copy.unit_type_id;
    world.catalog.register(copy);
    id
}

/// P-9 (the reveal's end, B-55 bounded): an air attacker over a plateau that strikes once is
/// revealed to the struck unit's player for its reveal ticks; the struck unit below, holding,
/// answers it meanwhile, and stops when the reveal ends, since its player's ground sight does
/// not reach the plateau.
#[test]
fn the_reveals_end_ends_the_answer() -> TestResult {
    let mut world = sim_fixtures::world(plateau_map()?, 0xB5D);
    let (air, mut strike) = world
        .catalog
        .archetypes()
        .filter(|arch| arch.mover_type == MoverType::Air && arch.max_speed > SimScalar::ZERO)
        .find_map(|arch| {
            arch.weapon_against(MoverType::Ground, None)
                .filter(|w| {
                    matches!(
                        w.attack_visibility,
                        sim_types::AttackVisibility::Visible { .. }
                    ) && w.splash.is_none()
                })
                .map(|w| (arch.clone(), w.clone()))
        })
        .ok_or("a shipped air unit with a Visible weapon against ground")?;
    let (ground, answer) = world
        .catalog
        .archetypes()
        .filter(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
        })
        .find_map(|arch| {
            arch.weapon_against(MoverType::Air, None)
                .filter(|w| w.max_range >= SimScalar::from_i32(4))
                .map(|w| (arch.clone(), w.clone()))
        })
        .ok_or("a shipped ground unit that hits air")?;
    // One strike: a period longer than the test.
    strike.period_ticks = 2_000;
    let reach =
        strike.max_range.min(answer.max_range) + air.collision_radius + ground.collision_radius
            - SimScalar::from_num(0.25);
    let air_type = register_armed(&mut world, &air, strike);
    let air_x = SimScalar::from_num(f64::from(PLATEAU) + 0.5);
    let row = SimScalar::from_num(16.5);
    let attacker = world.spawn_unit(air_type, PlayerId(0), SimVec2::new(air_x, row))?;
    let victim = world.spawn_unit(
        ground.unit_type_id,
        PlayerId(1),
        SimVec2::new(air_x - reach, row),
    )?;
    for (id, order) in [
        (victim, UnitOrder::HoldPosition),
        (attacker, UnitOrder::attack(victim)),
    ] {
        world
            .entities
            .get_mut(&id)
            .ok_or("spawned")?
            .issue_order(order, false)
            .map_err(|error| format!("{error:?}"))?;
    }
    let vitals = |world: &SimWorld| world.entities.get(&attacker).map(|e| (e.hp, e.shields));
    let full = vitals(&world);
    let mut answered = false;
    let mut tick = 0;
    while world.entities[&attacker].attack_reveal_ticks[sim_types::PlayerSlot::ALL[1]] == 0 {
        tick += 1;
        assert!(tick < 200, "the attacker strikes within 200 ticks");
        world.step(&bundle(tick));
    }
    while world.entities[&attacker].is_attack_revealed_to(PlayerId(1)) {
        tick += 1;
        world.step(&bundle(tick));
        answered |= vitals(&world) != full;
    }
    assert!(answered, "the victim answers the revealed attacker");
    let at_end = vitals(&world);
    for later in tick + 1..tick + 80 {
        world.step(&bundle(later));
        assert_eq!(
            vitals(&world),
            at_end,
            "tick {later}: no answer once the reveal ended"
        );
    }
    assert_eq!(
        world.entities[&victim].current_target, None,
        "the hold target is released"
    );
    Ok(())
}

/// Plan Amendment 2 (B-195), on both movement pipelines: a target last seen where the walker
/// cannot go (on a plateau with no ramp, seen by a spotter that then leaves) is walked to the
/// nearest point the walker can reach, which the sighting keeps; the order ends there, at the
/// foot of the cliff.
#[test]
fn an_unreachable_last_seen_point_is_replaced_by_the_nearest_reachable_one() -> TestResult {
    for pipeline in [MovementPipeline::Solver, MovementPipeline::Legacy] {
        let mut world = sim_fixtures::world(plateau_map()?, 0xB5F);
        on_pipeline(&mut world, pipeline)?;
        let (arch, mut weapon) = instant_archetype(&world)?;
        weapon.max_range = arch.sight_radius + SimScalar::from_i32(6);
        weapon.min_range = SimScalar::ZERO;
        let shooter_type = register_armed(&mut world, &arch, weapon);
        let dummy = register_blinker(&mut world)?;
        let row = 16.5;
        let shooter = world.spawn_unit(shooter_type, PlayerId(0), point(6.5, row))?;
        let target = world.spawn_unit(dummy, PlayerId(1), point(f64::from(PLATEAU) + 2.5, row))?;
        let spotter = world.spawn_unit(
            dummy,
            PlayerId(0),
            point(f64::from(PLATEAU) + 4.5, row + 2.0),
        )?;
        for (id, order) in [
            (target, UnitOrder::HoldPosition),
            (shooter, UnitOrder::attack(target)),
        ] {
            world
                .entities
                .get_mut(&id)
                .ok_or("spawned")?
                .issue_order(order, false)
                .map_err(|error| format!("{error:?}"))?;
        }
        steps(&mut world, 1, 3);
        assert_eq!(
            world.entities[&shooter]
                .movement
                .last_seen
                .map(|seen| seen.lost),
            Some(false),
            "{pipeline:?}: the spotter shows the target"
        );
        assert!(
            world.execute_ability(spotter, 0, Some(point(2.5, 2.5)), None),
            "{pipeline:?}: the spotter blinks below"
        );
        let mut replaced = None;
        let mut ended = false;
        for tick in 4..400 {
            world.step(&bundle(tick));
            let unit = &world.entities[&shooter];
            if !matches!(
                unit.order_queue.front(),
                Some(UnitOrder::AttackTarget { .. })
            ) {
                ended = true;
                break;
            }
            if let Some(seen) = unit.movement.last_seen.filter(|seen| seen.lost) {
                if seen.point.x < SimScalar::from_num(PLATEAU) {
                    replaced = Some(seen.point);
                }
            }
        }
        assert!(ended, "{pipeline:?}: the order ends");
        let replaced =
            replaced.ok_or_else(|| format!("{pipeline:?}: the point is replaced below"))?;
        let at = world.entities[&shooter].position;
        assert!(
            at.x < SimScalar::from_num(PLATEAU)
                && at.x > SimScalar::from_num(f64::from(PLATEAU) - 2.0),
            "{pipeline:?}: the shooter stops at the foot of the cliff, at {at:?}, the point {replaced:?}"
        );
    }
    Ok(())
}

/// P-10 (the spotter rule): a sieged unit whose siege weapon outranges its sight does not shell
/// an enemy in reach beyond its sight, and its attack order on it ends; once another unit of its
/// player sees the enemy, it shells it.
#[test]
fn a_sieged_tank_needs_a_spotter_beyond_its_sight() -> TestResult {
    let map = GameMap::new(ARENA, ARENA, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 0xB5E);
    let (tank, mode_ability) = world
        .catalog
        .archetypes()
        .find_map(|arch| {
            arch.modes.iter().find_map(|mode| {
                let outranges = matches!(
                    &mode.ground_weapon_override,
                    Some(Some(weapon)) if weapon.max_range > arch.sight_radius
                );
                let index = arch.abilities.iter().position(|ability| {
                    matches!(&ability.effect, AbilityEffectSpec::SwitchMode { mode_name } if *mode_name == mode.mode_name)
                })?;
                outranges.then(|| (arch.clone(), u8::try_from(index).ok()))
            })
        })
        .ok_or("a shipped mode weapon outranges its unit's sight")?;
    let mode_ability = mode_ability.ok_or("few abilities")?;
    let dummy = register_blinker(&mut world)?;
    let tank_id = world.spawn_unit(
        tank.unit_type_id,
        PlayerId(0),
        cell_centre(SHOOTER_CELL.0, SHOOTER_CELL.1),
    )?;
    assert!(
        world.execute_ability(tank_id, mode_ability, None, None),
        "the siege is admitted"
    );
    let mut tick = 0;
    while world.entities[&tank_id].active_mode.is_none()
        || world.entities[&tank_id].channeling_remaining_ticks > 0
    {
        tick += 1;
        assert!(tick < 200, "the tank sieges within 200 ticks");
        world.step(&bundle(tick));
    }
    let sight_cells = tank.sight_radius.saturating_to_u32();
    let target = world.spawn_unit(
        dummy,
        PlayerId(1),
        cell_centre(SHOOTER_CELL.0 + sight_cells + 1, SHOOTER_CELL.1),
    )?;
    let vitals = |world: &SimWorld| world.entities.get(&target).map(|e| (e.hp, e.shields));
    let full = vitals(&world);
    world
        .entities
        .get_mut(&tank_id)
        .ok_or("the tank exists")?
        .issue_order(UnitOrder::attack(target), false)
        .map_err(|error| format!("{error:?}"))?;
    for later in tick + 1..tick + 80 {
        world.step(&bundle(later));
        assert_eq!(
            vitals(&world),
            full,
            "tick {later}: nothing shells the unseen target"
        );
    }
    assert!(
        !matches!(
            world.entities[&tank_id].order_queue.front(),
            Some(UnitOrder::AttackTarget { .. })
        ),
        "the sieged tank's order on the unseen target ends"
    );
    world.spawn_unit(
        dummy,
        PlayerId(0),
        cell_centre(SHOOTER_CELL.0 + sight_cells + 3, SHOOTER_CELL.1 + 1),
    )?;
    tick += 80;
    for later in tick..tick + 150 {
        world.step(&bundle(later));
        if vitals(&world) != full {
            return Ok(());
        }
    }
    Err("the tank shells the target once a spotter sees it".into())
}

/// How the target of [`walk_after_a_loss`] is lost between the combat phase of tick T and the
/// movement phase of tick T + 1 (plan `2026-11-25-wind-up-and-hold-facing` A-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loss {
    /// Not lost: the control, a live swing.
    Kept,
    /// The target boards a transport.
    Boards,
    /// The target becomes cloaked, and no detector of the attacker's player covers it.
    Cloaks,
    /// The only unit of the attacker's player that saw the target dies, so the vision update
    /// of T + 1 leaves the target in fog.
    LosesItsSpotter,
}

/// A-3: the distance, in world units, a shooter under a move order walks on tick T + 1 when the
/// target of its wind-up (weapon `Stopped`, damage point 6) is lost as `loss` says after tick T.
///
/// The target, an unarmed dummy of player 1, stands beyond the shooter's sight, seen through a
/// spotter of player 0 beside it; a dummy of player 1 waits to carry it. The shooter walks two
/// ticks, is put into a wind-up against the target, the loss happens, and one full tick runs.
fn walk_after_a_loss(loss: Loss) -> Result<SimScalar, Box<dyn std::error::Error>> {
    let mut arena = arena(0xA3, instant_archetype, |weapon| {
        weapon.allowed_movement = sim_types::AllowedMovement::Stopped;
        weapon.damage_point_ticks = 6;
        weapon.period_ticks = weapon.period_ticks.max(12);
    })?;
    let unseen = arena.unseen();
    let target = arena.dummy_at(1, unseen)?;
    let spotter = arena.dummy_at(0, ahead(arena.sight_cells + 4))?;
    let carrier = arena.dummy_at(
        1,
        cell_centre(SHOOTER_CELL.0 + arena.sight_cells + 2, SHOOTER_CELL.1 + 3),
    )?;
    arena.order(
        arena.shooter,
        UnitOrder::Move {
            target: cell_centre(2, SHOOTER_CELL.1),
        },
    )?;
    steps(&mut arena.world, 1, 2);
    let before = {
        let shooter = arena
            .world
            .entities
            .get_mut(&arena.shooter)
            .ok_or("the shooter")?;
        shooter.weapon_phase = sim_core::WeaponPhase::Windup {
            target_id: target,
            remaining_ticks: 6,
        };
        shooter.position
    };
    assert!(
        arena
            .world
            .is_entity_available_for_combat(PlayerId(0), target),
        "premise: the spotter shows the target at the end of tick T"
    );
    match loss {
        Loss::Kept => {}
        Loss::Boards => {
            arena.world.try_load_unit_into_transport(target, carrier)?;
        }
        Loss::Cloaks => {
            arena
                .world
                .entities
                .get_mut(&target)
                .ok_or("the target")?
                .is_cloaked = true;
        }
        Loss::LosesItsSpotter => {
            arena
                .world
                .entities
                .get_mut(&spotter)
                .ok_or("the spotter")?
                .hp = SimScalar::ZERO;
            arena.world.reap_dead_and_emit_shatter();
        }
    }
    arena.world.step(&bundle(3));
    assert_eq!(arena.world.diagnostics(), []);
    Ok(arena.world.entities[&arena.shooter]
        .position
        .distance(before))
}

/// A-3 of plan `2026-11-25-wind-up-and-hold-facing` (D-1, B-207): a wind-up whose target is
/// lost before the tick starts (it boards a transport, cloaks unrevealed, or leaves its player's
/// sight when the spotter dies) has ended before the movement phase and holds nothing: the
/// shooter walks on under its move order. A live swing still holds it (the control).
#[test]
fn a_wind_up_whose_target_is_lost_before_the_tick_holds_nothing() -> TestResult {
    assert_eq!(
        walk_after_a_loss(Loss::Kept)?,
        SimScalar::ZERO,
        "the live swing of a `Stopped` weapon holds its unit"
    );
    for loss in [Loss::Boards, Loss::Cloaks, Loss::LosesItsSpotter] {
        assert!(
            walk_after_a_loss(loss)? > SimScalar::ZERO,
            "{loss:?}: the lost swing holds nothing"
        );
    }
    Ok(())
}

/// A-5 of plan `2026-11-25-wind-up-and-hold-facing`: the shooter's weapon phase, cooldown and
/// the world generator's position after tick T + 1, when the target of its wind-up was killed
/// and reaped on tick T (`in_wind_up`), or when the shooter never started a swing. The weapon
/// draws a random delay on every release.
fn after_a_lost_swing(
    in_wind_up: bool,
) -> Result<(sim_core::WeaponPhase, u16, u128), Box<dyn std::error::Error>> {
    let mut arena = arena(0xA5, instant_archetype, |weapon| {
        weapon.damage_point_ticks = 6;
        weapon.period_ticks = weapon.period_ticks.max(12);
        weapon.random_delay_min = SimScalar::ZERO;
        weapon.random_delay_max = SimScalar::from_i32(2);
    })?;
    let seen = arena.seen();
    let victim = arena.dummy_at(1, seen)?;
    arena.dummy_at(1, cell_centre(ARENA - 2, ARENA - 2))?;
    steps(&mut arena.world, 1, 2);
    arena
        .world
        .entities
        .get_mut(&victim)
        .ok_or("the victim")?
        .hp = SimScalar::ZERO;
    arena.world.reap_dead_and_emit_shatter();
    if in_wind_up {
        arena
            .world
            .entities
            .get_mut(&arena.shooter)
            .ok_or("the shooter")?
            .weapon_phase = sim_core::WeaponPhase::Windup {
            target_id: victim,
            remaining_ticks: 6,
        };
    }
    arena.world.step(&bundle(3));
    let shooter = &arena.world.entities[&arena.shooter];
    Ok((
        shooter.weapon_phase.clone(),
        shooter.weapon_cooldown_remaining,
        arena.world.rng.get_word_pos(),
    ))
}

/// A-5 of plan `2026-11-25-wind-up-and-hold-facing` (D-1): a swing whose target was killed
/// before its release ends in `Ready` and charges nothing: no cooldown and no random delay
/// draw; the generator stands where it stands in a world where the swing never started.
#[test]
fn a_lost_swing_charges_no_cooldown_and_draws_nothing() -> TestResult {
    let lost = after_a_lost_swing(true)?;
    let never = after_a_lost_swing(false)?;
    assert_eq!(lost.0, sim_core::WeaponPhase::Ready);
    assert_eq!(lost, never);
    Ok(())
}
