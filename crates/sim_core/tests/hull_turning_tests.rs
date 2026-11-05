//! Behavioural tests for hull turning: a hull turns at most its archetype's rate per tick.
//!
//! # Responsibility
//! Pins the rule of plan `2026-11-04-combat-sc2-alignment` D-8 item 4 (Q0045 1b). Every hull
//! turn goes through `Entity::turn_hull_towards`, which spends a per-tick budget set to the
//! archetype's `turn_rate` at the start of the movement phase. So the movement phase (facing
//! the velocity or the goal) and the weapon phase (aiming a hull-mounted weapon) together turn
//! the hull at most `turn_rate` in one tick, a hull-mounted weapon waits for the hull, a turret
//! turns at its own rate in the weapon phase only, and translation is never gated by facing.
//!
//! # How it fits
//! The tests drive the public world API (`SimWorld::step`) on a flat map and read headings,
//! turret angles and weapon phases between ticks, as a battle sees them.
//!
//! # Invariants
//! Every archetype is a scratch archetype written in this file, never a shipped identity, so a
//! rebalance of the shipped catalog cannot break these tests. Angles are `SimAngle` binary
//! angles: 65 536 units per turn, counter-clockwise from +x in the world frame. Rates are
//! authored in degrees per tick and read back from the catalog in binary units per tick.

use sim_core::catalog::UnitCatalog;
use sim_core::{SimWorld, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The scratch shooter's slug.
const SHOOTER: &str = "turning_shooter";
/// The scratch target's slug.
const TARGET: &str = "turning_target";

/// How the scratch shooter turns, moves and carries its weapon.
#[derive(Debug, Clone, Copy)]
struct Shooter {
    /// Hull rate, degrees per tick.
    turn_rate: f32,
    /// Speed, world units per tick; 0 makes the shooter immobile.
    max_speed: f32,
    /// Firing arc as the catalog reads it: a half angle in degrees.
    arc: f32,
    /// Turret rate in degrees per tick, or `None` for a hull-mounted weapon.
    turret: Option<f32>,
}

/// One scratch unit in the catalog's RON form.
fn unit_ron(slug: &str, id: u16, turn_rate: f32, max_speed: f32, weapon: &str) -> String {
    format!(
        r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of hull_turning_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: Ground,
            attributes: (bits: 1),
            max_hp: 10000.0,
            max_shields: 0.0,
            max_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: {max_speed:.3},
            acceleration: {max_speed:.3},
            deceleration: {max_speed:.3},
            turn_rate_deg_per_tick: {turn_rate:.2},
            collision_radius: 0.5,
            separation_radius: 0.5,
            push_priority: 20,
            sight_radius: 12.0,
            scan_range: 12.0,
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 1,
            supply_provided: 0,
            build_time_ticks: 100,
            weapon: {weapon},
            is_detector: false,
        )"#
    )
}

/// The scratch catalog: `shooter` with a 6-unit, 1-damage weapon (period 20, damage point 2),
/// and a target that turns at 180° per tick and moves at 0.1 per tick.
fn catalog_ron(shooter: Shooter) -> String {
    let mount = match shooter.turret {
        Some(rate) => format!("has_turret: true, turret_turn_rate_deg_per_tick: {rate:.2}"),
        None => "has_turret: false, turret_turn_rate_deg_per_tick: 0.0".to_owned(),
    };
    let weapon = format!(
        r#"Some((
            name: "Turning Rifle",
            visual_kind: Hitscan,
            target_flags: (bits: 255),
            min_range: 0.0,
            max_range: 6.0,
            firing_arc_deg: {arc:.2},
            {mount},
            base_damage: 1.0,
            bonus_armored: 0.0,
            bonus_light: 0.0,
            attacks_count: 1,
            period_ticks: 20,
            damage_point_ticks: 2,
            backswing_ticks: 0,
            splash: None,
            projectile_speed: None,
        ))"#,
        arc = shooter.arc,
    );
    format!(
        "(units: [{}, {}])",
        unit_ron(SHOOTER, 1, shooter.turn_rate, shooter.max_speed, &weapon),
        unit_ron(TARGET, 2, 180.0, 0.1, "None"),
    )
}

/// A shooter at (10, 16) facing +x and a target at (`target_x`, `target_y`), world units.
struct Bench {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
    tick: u32,
}

impl Bench {
    fn new(
        shooter: Shooter,
        target_x: i32,
        target_y: i32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = sim_fixtures::world(map, 7);
        world.catalog =
            UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &catalog_ron(shooter))?;
        let shooter_type = world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
        let target_type = world.catalog.id_by_slug(TARGET).ok_or("target")?;
        let at = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
        let shooter = world.spawn_unit(shooter_type, PlayerId(0), at(10, 16))?;
        let target = world.spawn_unit(target_type, PlayerId(1), at(target_x, target_y))?;
        Ok(Self {
            world,
            shooter,
            target,
            tick: 0,
        })
    }

    /// Steps one tick, issuing `order` to `unit` first when given.
    fn step(&mut self, order: Option<(EntityId, UnitOrder)>) {
        let commands = order
            .map(|(unit, order)| {
                vec![PlayerCommand {
                    player_id: self
                        .world
                        .entities
                        .get(&unit)
                        .map_or(PlayerId(0), |e| e.owner),
                    entities: vec![unit],
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

    /// The shooter's hull heading.
    fn heading(&self) -> Result<SimAngle, Box<dyn std::error::Error>> {
        Ok(self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter")?
            .heading)
    }

    /// The shooter's turret angle.
    fn turret(&self) -> Result<SimAngle, Box<dyn std::error::Error>> {
        Ok(self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter")?
            .turret_angle)
    }

    /// The shooter's weapon phase.
    fn phase(&self) -> Result<WeaponPhase, Box<dyn std::error::Error>> {
        Ok(self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter")?
            .weapon_phase
            .clone())
    }

    /// The shooter's position.
    fn position(&self) -> Result<SimVec2, Box<dyn std::error::Error>> {
        Ok(self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter")?
            .position)
    }

    /// The shooter's hull rate in binary units per tick, as the catalog read it.
    fn rate(&self) -> Result<u16, Box<dyn std::error::Error>> {
        let id = self.world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
        Ok(self.world.catalog.get(id)?.turn_rate.raw())
    }
}

/// The unsigned angle between two facings, in binary units.
fn turned(from: SimAngle, to: SimAngle) -> u16 {
    from.diff(to).unsigned_abs()
}

/// The facing from `from` to `to`.
fn facing(from: SimVec2, to: SimVec2) -> SimAngle {
    let delta = to - from;
    SimAngle::atan2(delta.y, delta.x)
}

#[test]
fn a_unit_facing_away_starts_its_swing_after_turning() -> TestResult {
    // 180° at 20.16° per tick: 8 ticks leave 18.72°, outside the 15° arc; the 9th aligns.
    let shooter = Shooter {
        turn_rate: 20.16,
        max_speed: 0.0,
        arc: 15.0,
        turret: None,
    };
    let mut bench = Bench::new(shooter, 7, 16)?;
    let target = bench.target;
    let mut first_swing = None;
    for tick in 1..=20_u32 {
        let order = (tick == 1).then(|| (bench.shooter, UnitOrder::attack(target)));
        bench.step(order);
        if first_swing.is_none() && matches!(bench.phase()?, WeaponPhase::Windup { .. }) {
            first_swing = Some(tick);
        }
    }
    assert_eq!(first_swing, Some(9));
    Ok(())
}

#[test]
fn a_hull_never_turns_more_than_its_rate_in_one_tick() -> TestResult {
    // A mobile shooter chases a target that walks away north: each tick the movement phase
    // faces the velocity or the goal and the weapon phase aims, and the two share one budget.
    let shooter = Shooter {
        turn_rate: 20.16,
        max_speed: 0.15,
        arc: 15.0,
        turret: None,
    };
    let mut bench = Bench::new(shooter, 4, 16)?;
    let rate = bench.rate()?;
    let (shooter_id, target) = (bench.shooter, bench.target);
    let north = SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(40));
    let mut before = bench.heading()?;
    let mut largest = 0;
    for tick in 0..120_u32 {
        let order = match tick {
            0 => Some((target, UnitOrder::Move { target: north })),
            1 => Some((shooter_id, UnitOrder::attack(target))),
            _ => None,
        };
        bench.step(order);
        let after = bench.heading()?;
        largest = largest.max(turned(before, after));
        before = after;
    }
    assert!(largest > 0, "the scenario must turn the hull");
    assert!(
        largest <= rate,
        "turned {largest} in one tick at rate {rate}"
    );
    Ok(())
}

#[test]
fn a_hull_weapon_turns_at_the_hull_rate_not_a_turret_rate() -> TestResult {
    let shooter = Shooter {
        turn_rate: 40.32,
        max_speed: 0.0,
        arc: 15.0,
        turret: None,
    };
    let mut bench = Bench::new(shooter, 7, 16)?;
    let target = bench.target;
    let start = bench.heading()?;
    bench.step(Some((bench.shooter, UnitOrder::attack(target))));
    assert_eq!(turned(start, bench.heading()?), bench.rate()?);
    assert_eq!(bench.rate()?, 7_340);
    Ok(())
}

#[test]
fn a_turret_turns_at_its_own_rate_while_the_hull_turns_at_its_rate() -> TestResult {
    // The target stands behind a mobile shooter, within range: the shooter stops to fire and
    // its turret, at 10° per tick (1 820 units), turns towards it; the hull is not asked to.
    let shooter = Shooter {
        turn_rate: 40.32,
        max_speed: 0.15,
        arc: 15.0,
        turret: Some(10.0),
    };
    let mut bench = Bench::new(shooter, 7, 16)?;
    let target = bench.target;
    let goal = facing(
        bench.position()?,
        SimVec2::new(SimScalar::from_i32(7), SimScalar::from_i32(16)),
    );
    let rate = bench.rate()?;
    let mut turret = bench.turret()?;
    let mut hull = bench.heading()?;
    let mut first_swing = None;
    for tick in 1..=30_u32 {
        let order = (tick == 1).then(|| (bench.shooter, UnitOrder::attack(target)));
        bench.step(order);
        let (now_turret, now_hull) = (bench.turret()?, bench.heading()?);
        let expected = turned(turret, goal).min(1_820);
        assert_eq!(
            turned(turret, now_turret),
            expected,
            "turret on tick {tick}"
        );
        assert!(turned(hull, now_hull) <= rate, "hull on tick {tick}");
        (turret, hull) = (now_turret, now_hull);
        if first_swing.is_none() && matches!(bench.phase()?, WeaponPhase::Windup { .. }) {
            first_swing = Some(tick);
        }
    }
    // 180° at 10° per tick: 16 ticks leave 20°, 17 leave 10°, within the 15° arc.
    assert_eq!(first_swing, Some(17));
    Ok(())
}

#[test]
fn a_moving_unit_keeps_its_path_while_its_heading_follows() -> TestResult {
    let shooter = Shooter {
        turn_rate: 20.16,
        max_speed: 0.15,
        arc: 15.0,
        turret: None,
    };
    let mut bench = Bench::new(shooter, 40, 40)?;
    let west = SimVec2::new(SimScalar::from_i32(2), SimScalar::from_i32(16));
    let start = bench.position()?;
    bench.step(Some((bench.shooter, UnitOrder::Move { target: west })));
    let moved = start.x - bench.position()?.x;
    assert!(
        moved > SimScalar::ZERO,
        "the first tick must move west, moved {moved:?}"
    );
    assert_eq!(turned(SimAngle::ZERO, bench.heading()?), bench.rate()?);
    Ok(())
}

#[test]
fn a_rate_of_180_turns_any_facing_in_one_tick() -> TestResult {
    let shooter = Shooter {
        turn_rate: 180.0,
        max_speed: 0.0,
        arc: 15.0,
        turret: None,
    };
    let mut bench = Bench::new(shooter, 7, 16)?;
    let target = bench.target;
    let goal = facing(
        bench.position()?,
        SimVec2::new(SimScalar::from_i32(7), SimScalar::from_i32(16)),
    );
    bench.step(Some((bench.shooter, UnitOrder::attack(target))));
    assert_eq!(bench.heading()?, goal);
    Ok(())
}

#[test]
fn turning_is_deterministic_and_its_budget_is_not_hashed() -> TestResult {
    let shooter = Shooter {
        turn_rate: 20.16,
        max_speed: 0.15,
        arc: 15.0,
        turret: None,
    };
    let mut first = Bench::new(shooter, 4, 20)?;
    let mut second = Bench::new(shooter, 4, 20)?;
    for tick in 0..40_u32 {
        let order =
            |bench: &Bench| (tick == 0).then(|| (bench.shooter, UnitOrder::attack(bench.target)));
        let (a, b) = (order(&first), order(&second));
        first.step(a);
        second.step(b);
        assert_eq!(
            first.world.last_checksum(),
            second.world.last_checksum(),
            "tick {tick}"
        );
    }
    let before = second.world.compute_checksum();
    let entity = second
        .world
        .entities
        .get_mut(&second.shooter)
        .ok_or("shooter")?;
    entity.hull_turn_budget = entity.hull_turn_budget.wrapping_add(1);
    assert_eq!(second.world.compute_checksum(), before);
    Ok(())
}
