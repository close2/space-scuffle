//! Behavioural tests for acceleration and deceleration (plan `2026-11-05-sc2-deviation-fixes`
//! WP12, B-39).
//!
//! # Responsibility
//! Pins the locomotion rule: a unit's own motion on a tick is at most the speed it achieved on
//! the tick before plus its archetype's `acceleration`, capped at its effective top speed; on
//! the way to a move order's destination it brakes so that its speed falls by at most its
//! `deceleration` per tick and it still stops on the destination; an order that ends without
//! arriving (Stop, Hold Position) lets it glide to rest at that rate, and a chase brakes into its
//! weapon's range (the owner's answer to Q0067). An `acceleration` or
//! `deceleration` at or above the archetype's `max_speed` changes speed within one tick, which
//! is how the catalog spells `StarCraft` II's `1000` ("instant").
//!
//! # How it fits
//! The tests drive the public world API (`SimWorld::step`) on a flat map and read the unit's
//! position and velocity between ticks, as a battle sees them.
//!
//! # Invariants
//! Every archetype is a scratch archetype written in this file, never a shipped identity, so a
//! rebalance of the shipped catalog cannot break these tests. Speeds are world units per tick and
//! accelerations world units per tick per tick.

use sim_core::catalog::UnitCatalog;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The scratch mover's slug.
const MOVER: &str = "accelerating_mover";

/// Tolerance for comparing fixed-point speeds with their exact values, world units per tick.
const EPSILON: f64 = 1e-6;

/// How the scratch mover moves.
#[derive(Debug, Clone, Copy)]
struct Motion {
    /// `Ground` or `Air`.
    mover: &'static str,
    /// Top speed, world units per tick.
    max_speed: f32,
    /// Speed gained per tick, world units per tick per tick.
    acceleration: f32,
    /// Speed lost per tick when braking, world units per tick per tick.
    deceleration: f32,
}

/// The scratch catalog holding one unarmed mover.
fn catalog_ron(motion: Motion) -> String {
    format!(
        r#"(units: [(
            slug: "{MOVER}",
            unit_type_id: 1,
            faction: "automaton_directorate",
            display_name: "{MOVER}",
            lore: "Scratch archetype of acceleration_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: {mover},
            attributes: (bits: 1),
            max_hp: 100.0,
            max_shields: 0.0,
            max_energy: 0.0,
            start_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: {max_speed},
            acceleration: {acceleration},
            deceleration: {deceleration},
            turn_rate_deg_per_tick: 180.0,
            collision_radius: 0.5,
            separation_radius: 0.5,
            push_priority: 20,
            sight_radius: 12.0,
            scan_range: 12.0,
            attack_target_priority: 20,
            primary_weapon: Either,
            acquire_tie_break: Closest,
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 1,
            supply_provided: 0,
            build_time_ticks: 100,
            weapon: None,
            is_detector: false,
        )])"#,
        mover = motion.mover,
        max_speed = motion.max_speed,
        acceleration = motion.acceleration,
        deceleration = motion.deceleration,
    )
}

/// One mover at (8, 24) on a flat 64 × 48 map, ordered to move east to (`goal_x`, 24).
struct Bench {
    world: SimWorld,
    mover: EntityId,
    tick: u32,
}

impl Bench {
    fn new(motion: Motion, goal_x: i32) -> Result<Self, Box<dyn std::error::Error>> {
        let map = GameMap::new(64, 48, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = sim_fixtures::world(map, 11);
        world.catalog =
            UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &catalog_ron(motion))?;
        let mover_type = world.catalog.id_by_slug(MOVER).ok_or("mover")?;
        let at = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
        let mover = world.spawn_unit(mover_type, PlayerId(0), at(8, 24))?;
        let mut bench = Self {
            world,
            mover,
            tick: 0,
        };
        bench.step(Some(UnitOrder::Move {
            target: at(goal_x, 24),
        }));
        Ok(bench)
    }

    /// Steps one tick, issuing `order` to the mover first when given.
    fn step(&mut self, order: Option<UnitOrder>) {
        let commands = order
            .map(|order| {
                vec![PlayerCommand {
                    player_id: PlayerId(0),
                    entities: vec![self.mover],
                    order,
                    queue: false,
                }]
            })
            .unwrap_or_default();
        self.world.step(&TickBundle {
            tick: Tick(self.tick),
            commands,
            state_checksum: None,
        });
        self.tick += 1;
    }

    /// The mover's speed on the last tick, world units per tick.
    fn speed(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let velocity = self
            .world
            .entities
            .get(&self.mover)
            .ok_or("mover")?
            .velocity;
        Ok(f64::from(velocity.length().to_f32()))
    }

    /// The mover's position along x, world units.
    fn x(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let position = self
            .world
            .entities
            .get(&self.mover)
            .ok_or("mover")?
            .position;
        Ok(f64::from(position.x.to_f32()))
    }

    /// Whether the mover still carries an order.
    fn has_order(&self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(!self
            .world
            .entities
            .get(&self.mover)
            .ok_or("mover")?
            .order_queue
            .is_empty())
    }

    /// Steps until the mover's order ends, at most `limit` ticks, recording each tick's speed.
    fn speeds_until_arrival(&mut self, limit: u32) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        let mut speeds = vec![self.speed()?];
        for _ in 0..limit {
            if !self.has_order()? {
                return Ok(speeds);
            }
            self.step(None);
            speeds.push(self.speed()?);
        }
        Err(format!("the mover did not arrive within {limit} ticks: {speeds:?}").into())
    }
}

/// A ground mover with acceleration 0.02 per tick², top speed 0.1, instant braking.
const GROUND_SLOW_START: Motion = Motion {
    mover: "Ground",
    max_speed: 0.1,
    acceleration: 0.02,
    deceleration: 0.1,
};

#[test]
fn a_unit_at_rest_gains_at_most_its_acceleration_per_tick() -> TestResult {
    let mut bench = Bench::new(GROUND_SLOW_START, 40)?;
    let mut speeds = vec![bench.speed()?];
    for _ in 0..6 {
        bench.step(None);
        speeds.push(bench.speed()?);
    }
    let expected = [0.02, 0.04, 0.06, 0.08, 0.1, 0.1, 0.1];
    for (tick, (speed, want)) in speeds.iter().zip(expected).enumerate() {
        assert!(
            (speed - want).abs() < EPSILON,
            "tick {tick}: speed {speed}, expected {want} (all {speeds:?})"
        );
    }
    Ok(())
}

#[test]
fn an_air_unit_accelerates_the_same_way() -> TestResult {
    let motion = Motion {
        mover: "Air",
        ..GROUND_SLOW_START
    };
    let mut bench = Bench::new(motion, 40)?;
    let first = bench.speed()?;
    bench.step(None);
    let second = bench.speed()?;
    assert!((first - 0.02).abs() < EPSILON, "first tick {first}");
    assert!((second - 0.04).abs() < EPSILON, "second tick {second}");
    Ok(())
}

#[test]
fn acceleration_at_or_above_top_speed_is_instant() -> TestResult {
    let motion = Motion {
        acceleration: 0.1,
        ..GROUND_SLOW_START
    };
    let bench = Bench::new(motion, 40)?;
    let speed = bench.speed()?;
    assert!((speed - 0.1).abs() < EPSILON, "first tick {speed}");
    Ok(())
}

#[test]
fn instant_deceleration_arrives_at_full_speed() -> TestResult {
    let motion = Motion {
        acceleration: 0.1,
        ..GROUND_SLOW_START
    };
    let mut bench = Bench::new(motion, 12)?;
    let speeds = bench.speeds_until_arrival(200)?;
    let moving: Vec<f64> = speeds.iter().copied().filter(|s| *s > 0.0).collect();
    let before_last = moving.len().saturating_sub(2);
    assert!(
        moving[..before_last]
            .iter()
            .all(|s| (s - 0.1).abs() < EPSILON),
        "speeds {speeds:?}"
    );
    assert!(
        (bench.x()? - 12.0).abs() < EPSILON,
        "stopped at {}",
        bench.x()?
    );
    Ok(())
}

#[test]
fn finite_deceleration_brakes_into_the_destination() -> TestResult {
    let motion = Motion {
        mover: "Air",
        max_speed: 0.1,
        acceleration: 0.1,
        deceleration: 0.01,
    };
    let mut bench = Bench::new(motion, 12)?;
    let speeds = bench.speeds_until_arrival(200)?;
    let mut braking = false;
    // The last recorded tick snaps onto the goal, covering whatever distance was left.
    let (last, run) = speeds.split_last().ok_or("no speeds")?;
    assert!(*last <= 0.01 + EPSILON, "arrival step {last}: {speeds:?}");
    for pair in run.windows(2) {
        let drop = pair[0] - pair[1];
        if drop > EPSILON {
            braking = true;
        }
        assert!(
            drop <= 0.01 + EPSILON,
            "speed fell by {drop} in one tick: {speeds:?}"
        );
    }
    assert!(braking, "the mover never slowed: {speeds:?}");
    assert!(
        (bench.x()? - 12.0).abs() < EPSILON,
        "stopped at {}",
        bench.x()?
    );
    Ok(())
}

/// An air mover at top speed 0.1 at once, braking at 0.01 per tick².
const AIR_GLIDER: Motion = Motion {
    mover: "Air",
    max_speed: 0.1,
    acceleration: 0.1,
    deceleration: 0.01,
};

/// Moves `motion` east at full speed for 10 ticks, then gives `order`; returns the speeds and x
/// positions of the 15 ticks after the order, the order's tick first.
fn speeds_after(
    motion: Motion,
    order: UnitOrder,
) -> Result<(Vec<f64>, Vec<f64>), Box<dyn std::error::Error>> {
    let mut bench = Bench::new(motion, 40)?;
    for _ in 0..10 {
        bench.step(None);
    }
    let mut speeds = Vec::new();
    let mut xs = Vec::new();
    bench.step(Some(order));
    for _ in 0..15 {
        speeds.push(bench.speed()?);
        xs.push(bench.x()?);
        bench.step(None);
    }
    Ok((speeds, xs))
}

#[test]
fn q0067_an_order_that_ends_without_arriving_glides_to_rest_at_the_deceleration() -> TestResult {
    // The owner's answer to Q0067, option 2: braking extends to orders that end without
    // arriving. Stop and Hold Position end the move short of its destination.
    for order in [UnitOrder::Stop, UnitOrder::HoldPosition] {
        let (speeds, xs) = speeds_after(AIR_GLIDER, order.clone())?;
        let expected = [0.09, 0.08, 0.07, 0.06, 0.05, 0.04, 0.03, 0.02, 0.01, 0.0];
        for (tick, (speed, want)) in speeds.iter().zip(expected).enumerate() {
            assert!(
                (speed - want).abs() < EPSILON,
                "{order:?} tick {tick}: speed {speed}, expected {want} (all {speeds:?})"
            );
        }
        assert!(
            speeds[expected.len()..].iter().all(|s| *s == 0.0),
            "{order:?}: at rest, it stays: {speeds:?}"
        );
        // It glides on along its heading: 0.09 + 0.08 + … + 0.01 = 0.45 past the order's point.
        let glide = xs[9] - (xs[0] - 0.09);
        assert!(
            (glide - 0.45).abs() < 1e-4,
            "{order:?}: glided {glide} ({xs:?})"
        );
    }
    Ok(())
}

#[test]
fn q0067_a_pushed_unit_at_rest_does_not_glide() -> TestResult {
    // Collision writes a pushed body's displacement into its velocity; a push is not momentum
    // of the unit's own, so a standing unit does not carry it on.
    let mut bench = Bench::new(AIR_GLIDER, 40)?;
    bench.step(Some(UnitOrder::Stop));
    for _ in 0..15 {
        bench.step(None);
    }
    assert!(bench.speed()? == 0.0, "at rest after its glide");
    let mover = bench.world.entities.get_mut(&bench.mover).ok_or("mover")?;
    mover.velocity = SimVec2::new(SimScalar::from_num(0.05), SimScalar::ZERO);
    let x = bench.x()?;
    bench.step(None);
    assert!((bench.x()? - x).abs() < EPSILON, "a push does not carry on");
    assert!(bench.speed()? == 0.0, "{}", bench.speed()?);
    Ok(())
}

#[test]
fn q0067_instant_deceleration_still_stops_within_a_tick() -> TestResult {
    let motion = Motion {
        deceleration: 0.1,
        ..AIR_GLIDER
    };
    for order in [UnitOrder::Stop, UnitOrder::HoldPosition] {
        let (speeds, xs) = speeds_after(motion, order.clone())?;
        assert!(speeds.iter().all(|s| *s == 0.0), "{order:?}: {speeds:?}");
        assert!(
            xs.windows(2).all(|w| (w[0] - w[1]).abs() < EPSILON),
            "{order:?}: {xs:?}"
        );
    }
    Ok(())
}

/// A world on the shipped catalog with a chaser cloned from the first shipped armed mobile
/// unit (radius 0.5, top speed 0.1 reached at once, braking at `deceleration`, every weapon's
/// reach 2) at (8, 24), attacking a passive, holding target at (20, 24). Returns the world and
/// both ids.
fn pursuit_world(
    deceleration: f32,
) -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
    let map = GameMap::new(64, 48, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 11);
    let base = world
        .catalog
        .archetypes()
        .find(|arch| arch.weapon.is_some() && arch.max_speed > SimScalar::ZERO)
        .ok_or("a shipped armed unit")?
        .clone();
    let mut free = (1..=u16::MAX)
        .map(sim_types::UnitTypeId)
        .filter(|id| world.catalog.get(*id).is_err());
    let (chaser_type, target_type) = (free.next().ok_or("id")?, free.next().ok_or("id")?);
    let mut chaser = base.clone();
    chaser.unit_type_id = chaser_type;
    chaser.slug = "braking_chaser".to_string();
    chaser.collision_radius = SimScalar::from_num(0.5);
    chaser.max_speed = SimScalar::from_num(0.1);
    chaser.acceleration = SimScalar::from_num(0.1);
    chaser.deceleration = SimScalar::from_f32_lossy(deceleration);
    chaser.turn_before_move = false;
    chaser.capabilities.attack_dash = None;
    for weapon in [
        &mut chaser.weapon,
        &mut chaser.ground_weapon,
        &mut chaser.air_weapon,
    ]
    .into_iter()
    .flatten()
    {
        weapon.max_range = SimScalar::from_num(2);
        weapon.min_range = SimScalar::ZERO;
    }
    let mut target = base;
    target.unit_type_id = target_type;
    target.slug = "braking_target".to_string();
    target.collision_radius = SimScalar::from_num(0.5);
    target.weapon = None;
    target.ground_weapon = None;
    target.air_weapon = None;
    target.max_hp = SimScalar::from_num(100_000);
    world.catalog.register(chaser);
    world.catalog.register(target);
    let at = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    let chaser = world.spawn_unit(chaser_type, PlayerId(0), at(8, 24))?;
    let target = world.spawn_unit(target_type, PlayerId(1), at(20, 24))?;
    world
        .entities
        .get_mut(&target)
        .ok_or("target")?
        .issue_order(UnitOrder::HoldPosition, false)?;
    world
        .entities
        .get_mut(&chaser)
        .ok_or("chaser")?
        .issue_order(UnitOrder::attack(target), false)?;
    Ok((world, chaser, target))
}

/// The chaser's speed each tick for 200 ticks, and its final centre distance to the target.
fn pursuit_speeds(deceleration: f32) -> Result<(Vec<f64>, f64), Box<dyn std::error::Error>> {
    let (mut world, chaser, target) = pursuit_world(deceleration)?;
    let mut speeds = Vec::new();
    for tick in 0..200 {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        let velocity = world.entities.get(&chaser).ok_or("chaser")?.velocity;
        speeds.push(f64::from(velocity.length().to_f32()));
    }
    let gap = world
        .entities
        .get(&chaser)
        .ok_or("chaser")?
        .position
        .distance(world.entities.get(&target).ok_or("target")?.position);
    Ok((speeds, f64::from(gap.to_f32())))
}

#[test]
fn q0067_pursuit_brakes_into_the_weapon_range() -> TestResult {
    // The owner's answer to Q0067, option 2: braking extends to pursuit. The chaser closes from
    // 12 to its reach of 2 + 0.5 + 0.5 = 3 and stops there, losing at most 0.01 a tick.
    let (speeds, gap) = pursuit_speeds(0.01)?;
    for pair in speeds.windows(2) {
        let drop = pair[0] - pair[1];
        assert!(
            drop <= 0.01 + EPSILON,
            "speed fell by {drop} in one tick: {speeds:?}"
        );
    }
    assert!(
        speeds.iter().any(|s| *s > 0.0 && *s < 0.1 - EPSILON),
        "it braked: {speeds:?}"
    );
    assert!(*speeds.last().ok_or("speeds")? == 0.0, "{speeds:?}");
    assert!(gap <= 3.0 + 1e-4, "in range: {gap}");
    assert!(gap >= 2.9, "not past its reach: {gap}");
    Ok(())
}

#[test]
fn q0067_instant_deceleration_pursuit_stops_within_a_tick() -> TestResult {
    let (speeds, gap) = pursuit_speeds(0.1)?;
    let stop = speeds
        .iter()
        .position(|s| *s == 0.0)
        .ok_or("the chaser stopped")?;
    assert!(
        speeds[..stop.saturating_sub(1)]
            .iter()
            .all(|s| (s - 0.1).abs() < EPSILON),
        "{speeds:?}"
    );
    assert!(gap <= 3.0 + 1e-4, "in range: {gap}");
    Ok(())
}
