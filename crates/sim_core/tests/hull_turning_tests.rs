//! Behavioural tests for hull turning: a hull turns at most its archetype's rate per tick.
//!
//! # Responsibility
//! Pins the rule of plan `2026-11-04-combat-sc2-alignment` D-8 item 4 (Q0045 1b). Every hull
//! turn goes through `Entity::turn_hull_towards`, which spends a per-tick budget set to the
//! archetype's `turn_rate` at the start of the movement phase. So the movement phase (facing
//! the velocity or the goal) and the weapon phase (aiming a hull-mounted weapon) together turn
//! the hull at most `turn_rate` in one tick, a hull-mounted weapon waits for the hull, a turret
//! turns at its own rate in the weapon phase only.
//!
//! Translation is gated by facing only for archetypes flagged `turn_before_move` (plan
//! `2026-11-05-turn-before-move`, Q0050): such a unit translates by its own motion on a tick only
//! if, after that tick's hull turn, its heading equals the direction of its step. While the step
//! lies further from its heading than one tick's turn, it turns in place and keeps its order.
//! Collision pushes are not gated. Every other archetype moves along its path at once while its
//! hull follows.
//!
//! A hull weapon that fires outside an attack order (plan `2026-11-23-turret-wind-up` D-5) never
//! turns the hull: the hull belongs to the unit's order, so the weapon fires only at a target
//! already in its arc.
//!
//! Between shots the weapon turns the hull of a unit that holds position or is immobile towards
//! its target, at what is left of the tick's budget, when it works for an attack order; it never
//! turns a hull that pursuit aims, and nothing turns with no target (plan
//! `2026-11-25-wind-up-and-hold-facing` D-4, B-199).
//!
//! # How it fits
//! The tests drive the public world API (`SimWorld::step`) on a flat map and read headings,
//! turret angles and weapon phases between ticks, as a battle sees them.
//!
//! # Invariants
//! Every archetype is a scratch archetype written in this file, never a shipped identity, so a
//! rebalance of the shipped catalog cannot break these tests. The one shipped-data test finds
//! its archetypes by property (`turn_before_move`), never by name. Angles are `SimAngle` binary
//! angles: 65 536 units per turn, counter-clockwise from +x in the world frame. Rates are
//! authored in degrees per tick and read back from the catalog in binary units per tick.

use sim_core::catalog::UnitCatalog;
use sim_core::{step_entity_weapon, Mandate, SimWorld, WeaponPhase, WeaponTarget};
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
    /// Whether the hull must face a step before the shooter takes it (`turn_before_move`).
    turn_before_move: bool,
    /// Hull rate in degrees per tick while it does not translate, or `None` for the hull rate.
    stationary: Option<f32>,
}

/// One scratch unit in the catalog's RON form.
fn unit_ron(
    slug: &str,
    id: u16,
    turn_rate: f32,
    max_speed: f32,
    turn_before_move: bool,
    stationary: Option<f32>,
    weapon: &str,
) -> String {
    let stationary = stationary.map_or(String::new(), |rate| {
        format!("stationary_turn_rate_deg_per_tick: Some({rate:.2}),")
    });
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
            start_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: {max_speed:.3},
            acceleration: {max_speed:.3},
            deceleration: {max_speed:.3},
            turn_rate_deg_per_tick: {turn_rate:.2},
            turn_before_move: {turn_before_move},
            {stationary}
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
            weapon: {weapon},
        )"#
    )
}

/// The scratch catalog: `shooter` with a 6-unit, 1-damage weapon (period 20, damage point 2),
/// and a target that turns at 180° per tick and moves at 0.1 per tick.
fn catalog_ron(shooter: Shooter) -> String {
    let mount = match shooter.turret {
        Some(rate) => {
            format!("has_turret: true, turret_turn_rate_deg_per_tick: {rate:.2}, turret_idle: Hold")
        }
        None => {
            "has_turret: false, turret_turn_rate_deg_per_tick: 0.0, turret_idle: Hold".to_owned()
        }
    };
    let weapon = format!(
        r#"Some((
            name: "Turning Rifle",
            visual_kind: Hitscan,
            target_flags: (bits: 255),
            min_range: 0.0,
            max_range: 6.0,
            firing_arc_deg: {arc:.2},
            acquire_arc_deg: {arc:.2},
            {mount},
            base_damage: 1.0,
            attribute_bonuses: [],
            attacks_count: 1,
            first_strike_delay_ticks: 0,
            strike_interval_ticks: 0,
            period_ticks: 20,
            damage_point_ticks: 2,
            backswing_ticks: 0,
            random_delay_ticks: (0.0, 0.0),
            splash: None,
            projectile_speed: None,
        ))"#,
        arc = shooter.arc,
    );
    format!(
        "(units: [{}, {}])",
        unit_ron(
            SHOOTER,
            1,
            shooter.turn_rate,
            shooter.max_speed,
            shooter.turn_before_move,
            shooter.stationary,
            &weapon,
        ),
        unit_ron(TARGET, 2, 180.0, 0.1, false, None, "None"),
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
        let commands = order.map_or_default(|(unit, order)| {
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
        });
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

    /// The shooter's velocity, world units per tick.
    fn velocity(&self) -> Result<SimVec2, Box<dyn std::error::Error>> {
        Ok(self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter")?
            .velocity)
    }

    /// The shooter's front order, if any.
    fn front_order(&self) -> Result<Option<UnitOrder>, Box<dyn std::error::Error>> {
        Ok(self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter")?
            .order_queue
            .front()
            .cloned())
    }

    /// The shooter's speed in world units per tick, as the catalog read it.
    fn speed(&self) -> Result<SimScalar, Box<dyn std::error::Error>> {
        let id = self.world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
        Ok(self.world.catalog.get(id)?.max_speed)
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
        turn_before_move: false,
        stationary: None,
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
        turn_before_move: false,
        stationary: None,
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
        turn_before_move: false,
        stationary: None,
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
        turn_before_move: false,
        stationary: None,
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
        turn_before_move: false,
        stationary: None,
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
        turn_before_move: false,
        stationary: None,
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
        turn_before_move: false,
        stationary: None,
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
    let mut pivots = [Bench::new(PIVOT, 40, 40)?, Bench::new(PIVOT, 40, 40)?];
    for tick in 0..20_u32 {
        for bench in &mut pivots {
            let order = (tick == 0).then(|| {
                (
                    bench.shooter,
                    UnitOrder::Move {
                        target: point(2, 16),
                    },
                )
            });
            bench.step(order);
        }
        assert_eq!(
            pivots[0].world.last_checksum(),
            pivots[1].world.last_checksum(),
            "flagged reversal, tick {tick}"
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

/// A mobile shooter with a hull weapon at 20.16° per tick that must face its steps.
const PIVOT: Shooter = Shooter {
    turn_rate: 20.16,
    max_speed: 0.15,
    arc: 15.0,
    turret: None,
    turn_before_move: true,
    stationary: None,
};

/// A world point from whole coordinates.
fn point(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y))
}

#[test]
fn a_turn_before_move_unit_turns_in_place_on_a_reversal() -> TestResult {
    // 180° at 3 670 units per tick: 8 ticks in place (29 360), the 9th fits the remaining 3 408.
    let mut bench = Bench::new(PIVOT, 40, 40)?;
    let (rate, speed, start) = (bench.rate()?, bench.speed()?, bench.position()?);
    let west = point(2, 16);
    let mut before = bench.heading()?;
    for tick in 1..=8_u32 {
        let order = (tick == 1).then_some((bench.shooter, UnitOrder::Move { target: west }));
        bench.step(order);
        assert_eq!(bench.position()?, start, "tick {tick} must not translate");
        // Zero velocity makes a turning unit "still" for the contact rules (plan D-4).
        assert_eq!(bench.velocity()?, SimVec2::ZERO, "tick {tick}");
        assert_eq!(turned(before, bench.heading()?), rate, "tick {tick}");
        before = bench.heading()?;
    }
    bench.step(None);
    assert_eq!(bench.heading()?, facing(start, west), "tick 9 faces west");
    assert_eq!(
        bench.position()?,
        start - SimVec2::new(speed, SimScalar::ZERO)
    );
    Ok(())
}

#[test]
fn a_turn_before_move_unit_does_not_stop_for_a_gentle_turn() -> TestResult {
    // (20, 5) from the shooter is 14.04° off its heading, within one tick's 20.16°.
    let mut bench = Bench::new(PIVOT, 40, 40)?;
    let start = bench.position()?;
    bench.step(Some((
        bench.shooter,
        UnitOrder::Move {
            target: point(30, 21),
        },
    )));
    assert_ne!(bench.position()?, start, "the first tick must translate");
    let velocity = bench.velocity()?;
    assert_eq!(bench.heading()?, SimAngle::atan2(velocity.y, velocity.x));
    Ok(())
}

#[test]
fn a_turn_before_move_unit_moves_on_the_tick_its_remaining_turn_fits() -> TestResult {
    // (12, 7) from the shooter is 30.26° off: one tick in place, then the remaining 10.1° fits.
    let mut bench = Bench::new(PIVOT, 40, 40)?;
    let (rate, start) = (bench.rate()?, bench.position()?);
    bench.step(Some((
        bench.shooter,
        UnitOrder::Move {
            target: point(22, 23),
        },
    )));
    assert_eq!(bench.position()?, start, "tick 1 turns in place");
    assert_eq!(turned(SimAngle::ZERO, bench.heading()?), rate);
    bench.step(None);
    assert_ne!(bench.position()?, start, "tick 2 translates");
    let velocity = bench.velocity()?;
    assert_eq!(bench.heading()?, SimAngle::atan2(velocity.y, velocity.x));
    Ok(())
}

#[test]
fn a_turn_before_move_unit_turns_before_it_chases_a_target_behind_it() -> TestResult {
    // The target stands 8 units behind, out of the 6-unit range (7 with both radii).
    let mut bench = Bench::new(PIVOT, 2, 16)?;
    let (rate, start, target) = (bench.rate()?, bench.position()?, bench.target);
    let mut before = bench.heading()?;
    for tick in 1..=8_u32 {
        let order = (tick == 1).then(|| (bench.shooter, UnitOrder::attack(target)));
        bench.step(order);
        assert_eq!(bench.position()?, start, "tick {tick} must not translate");
        assert_eq!(turned(before, bench.heading()?), rate, "tick {tick}");
        before = bench.heading()?;
    }
    bench.step(None);
    assert!(bench.position()?.x < start.x, "tick 9 closes on the target");
    let velocity = bench.velocity()?;
    assert_eq!(bench.heading()?, SimAngle::atan2(velocity.y, velocity.x));
    Ok(())
}

#[test]
fn turning_in_place_neither_finishes_nor_shortens_the_order() -> TestResult {
    // A goal 0.1 behind lies within one step, so an unflagged unit would arrive on tick 1.
    let mut bench = Bench::new(PIVOT, 40, 40)?;
    let start = bench.position()?;
    let goal = start - SimVec2::new(SimScalar::from_num(0.1), SimScalar::ZERO);
    let order = UnitOrder::Move { target: goal };
    let mut arrived = None;
    for tick in 1..=12_u32 {
        bench.step((tick == 1).then(|| (bench.shooter, order.clone())));
        if bench.position()? == goal {
            arrived = Some(tick);
            assert_eq!(bench.heading()?, facing(start, goal), "it faces the goal");
            assert_eq!(bench.front_order()?, None, "arriving ends the order");
            break;
        }
        assert_eq!(bench.position()?, start, "tick {tick}");
        assert_eq!(bench.front_order()?, Some(order.clone()), "tick {tick}");
    }
    assert_eq!(arrived, Some(9));
    Ok(())
}

#[test]
fn a_turn_before_move_unit_on_hold_position_turns_to_fire_and_never_moves() -> TestResult {
    let mut bench = Bench::new(PIVOT, 7, 16)?;
    let start = bench.position()?;
    let mut first_swing = None;
    for tick in 1..=20_u32 {
        let order = (tick == 1).then_some((bench.shooter, UnitOrder::HoldPosition));
        bench.step(order);
        assert_eq!(bench.position()?, start, "tick {tick}");
        if first_swing.is_none() && matches!(bench.phase()?, WeaponPhase::Windup { .. }) {
            first_swing = Some(tick);
        }
    }
    assert!(first_swing.is_some(), "the held unit turns and fires");
    Ok(())
}

/// The stationary rate of the scratch shooters below: 40.32° per tick, 7 340 binary units.
const STATIONARY: f32 = 40.32;

/// `STATIONARY` in binary units per tick.
const STATIONARY_UNITS: u16 = 7_340;

#[test]
fn a_standing_unit_aims_its_hull_at_the_stationary_rate() -> TestResult {
    let shooter = Shooter {
        turn_rate: 20.16,
        max_speed: 0.0,
        arc: 15.0,
        turret: None,
        turn_before_move: false,
        stationary: Some(STATIONARY),
    };
    let mut bench = Bench::new(shooter, 7, 16)?;
    let target = bench.target;
    bench.step(Some((bench.shooter, UnitOrder::attack(target))));
    assert_eq!(turned(SimAngle::ZERO, bench.heading()?), STATIONARY_UNITS);
    Ok(())
}

#[test]
fn a_moving_unit_turns_at_the_moving_rate() -> TestResult {
    let shooter = Shooter {
        turn_rate: 20.16,
        max_speed: 0.15,
        arc: 15.0,
        turret: None,
        turn_before_move: false,
        stationary: Some(STATIONARY),
    };
    let mut bench = Bench::new(shooter, 40, 40)?;
    let start = bench.position()?;
    bench.step(Some((
        bench.shooter,
        UnitOrder::Move {
            target: point(2, 16),
        },
    )));
    assert!(bench.position()?.x < start.x, "the unit translates");
    assert_eq!(turned(SimAngle::ZERO, bench.heading()?), bench.rate()?);
    Ok(())
}

#[test]
fn a_turn_before_move_unit_turns_in_place_at_the_stationary_rate() -> TestResult {
    // 180° at 7 340 units per tick in place: 4 ticks leave 3 408, which fits the moving 3 670.
    let shooter = Shooter {
        stationary: Some(STATIONARY),
        ..PIVOT
    };
    let mut bench = Bench::new(shooter, 40, 40)?;
    let (speed, start) = (bench.speed()?, bench.position()?);
    let west = point(2, 16);
    let mut before = bench.heading()?;
    for tick in 1..=4_u32 {
        let order = (tick == 1).then_some((bench.shooter, UnitOrder::Move { target: west }));
        bench.step(order);
        assert_eq!(bench.position()?, start, "tick {tick} must not translate");
        assert_eq!(
            turned(before, bench.heading()?),
            STATIONARY_UNITS,
            "tick {tick}"
        );
        before = bench.heading()?;
    }
    bench.step(None);
    assert_eq!(bench.heading()?, facing(start, west), "tick 5 faces west");
    assert_eq!(
        bench.position()?,
        start - SimVec2::new(speed, SimScalar::ZERO)
    );
    Ok(())
}

#[test]
fn shipped_turn_before_move_units_turn_in_place_on_a_reversal() -> TestResult {
    // Every shipped archetype with the flag, found by property: spawned facing +x and sent due
    // west, it turns in place at its stationary rate s until the rest of the half turn fits in
    // its moving rate r, ceil((32 768 - r) / s) ticks, and steps on the next.
    let catalog = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    )
    .catalog;
    let flagged: Vec<_> = catalog
        .archetypes()
        .filter(|archetype| archetype.turn_before_move)
        .map(|archetype| (archetype.slug.clone(), archetype.unit_type_id))
        .collect();
    assert!(!flagged.is_empty(), "the shipped catalog flags no unit");
    for (slug, unit_type) in flagged {
        let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = sim_fixtures::world(map, 7);
        let archetype = world.catalog.get(unit_type)?;
        let rate = u32::from(archetype.turn_rate.raw());
        let stationary = u32::from(archetype.stationary_turn_rate.raw());
        let in_place = u32::from(SimAngle::DEG_180.raw())
            .saturating_sub(rate)
            .div_ceil(stationary);
        let start = point(40, 32);
        let unit = world.spawn_unit(unit_type, PlayerId(0), start)?;
        for tick in 0..=in_place {
            let commands = if tick == 0 {
                vec![PlayerCommand {
                    player_id: PlayerId(0),
                    entities: vec![unit],
                    order: UnitOrder::Move {
                        target: point(4, 32),
                    },
                    queue: false,
                }]
            } else {
                Vec::new()
            };
            world.step(&TickBundle {
                tick: Tick(tick),
                commands,
                state_checksum: None,
            });
            let position = world.entities.get(&unit).ok_or("unit")?.position;
            if tick < in_place {
                assert_eq!(position, start, "{slug} tick {}", tick + 1);
            } else {
                assert!(position.x < start.x, "{slug} steps on tick {}", tick + 1);
            }
        }
    }
    Ok(())
}

#[test]
fn shipped_units_with_a_stationary_rate_turn_faster_standing_than_moving() -> TestResult {
    // Found by property: every shipped archetype whose stationary rate differs from its moving
    // rate. The game gives every such analogue a faster stationary rate (audit §10.2).
    let catalog = sim_fixtures::world(
        GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0)))?,
        1,
    )
    .catalog;
    let differing: Vec<_> = catalog
        .archetypes()
        .filter(|archetype| archetype.stationary_turn_rate != archetype.turn_rate)
        .map(|archetype| {
            (
                archetype.slug.clone(),
                archetype.turn_rate.raw(),
                archetype.stationary_turn_rate.raw(),
            )
        })
        .collect();
    assert!(
        !differing.is_empty(),
        "no shipped unit has a stationary rate of its own"
    );
    for (slug, moving, stationary) in differing {
        assert!(
            stationary > moving,
            "{slug}: stationary {stationary} against moving {moving}"
        );
    }
    Ok(())
}

/// The tick (counted from 1, the tick of the attack order) on which the shooter first starts a
/// swing against the target at (`target_x`, `target_y`), within 20 ticks.
fn first_swing_tick(
    shooter: Shooter,
    target_x: i32,
    target_y: i32,
) -> Result<Option<u32>, Box<dyn std::error::Error>> {
    let mut bench = Bench::new(shooter, target_x, target_y)?;
    let target = bench.target;
    for tick in 1..=20_u32 {
        let order = (tick == 1).then(|| (bench.shooter, UnitOrder::attack(target)));
        bench.step(order);
        if matches!(bench.phase()?, WeaponPhase::Windup { .. }) {
            return Ok(Some(tick));
        }
    }
    Ok(None)
}

#[test]
fn a_turn_that_aligns_the_weapon_starts_the_swing_on_that_tick() -> TestResult {
    // M-22 (plan `2026-11-05-sc2-deviation-fixes` WP1, B-1): the target stands 36.87° off the
    // hull (at (14, 19) from (10, 16)); one tick's turn of 55.99° brings it inside the 11.25°
    // arc, so the swing starts on that same tick, as StarCraft II turns and attacks in one
    // motion.
    let shooter = Shooter {
        turn_rate: 55.99,
        max_speed: 0.0,
        arc: 11.25,
        turret: None,
        turn_before_move: false,
        stationary: None,
    };
    assert_eq!(first_swing_tick(shooter, 14, 19)?, Some(1));
    Ok(())
}

#[test]
fn a_turn_that_does_not_align_waits() -> TestResult {
    // Guard: the target stands 126.87° off (at (7, 20)). After one tick's turn of 55.99° it is
    // 70.88° off and after two 14.89°, both outside the 11.25° arc; the third turn aligns.
    let shooter = Shooter {
        turn_rate: 55.99,
        max_speed: 0.0,
        arc: 11.25,
        turret: None,
        turn_before_move: false,
        stationary: None,
    };
    assert_eq!(first_swing_tick(shooter, 7, 20)?, Some(3));
    Ok(())
}

#[test]
fn a_turret_turn_that_aligns_the_weapon_starts_the_swing_on_that_tick() -> TestResult {
    // The same rule for a turret: the target stands 14.04° off (at (14, 17)); one tick's turret
    // turn of 10° leaves 4.04°, inside the 5° arc, so the swing starts on that tick.
    let shooter = Shooter {
        turn_rate: 55.99,
        max_speed: 0.0,
        arc: 5.0,
        turret: Some(10.0),
        turn_before_move: false,
        stationary: None,
    };
    assert_eq!(first_swing_tick(shooter, 14, 17)?, Some(1));
    Ok(())
}

/// A world for plan `2026-11-23-turret-wind-up` D-5 (P-4): the shooter at (10, 16) facing +x with
/// a hull weapon in its air slot (range 6, a 30° half arc) that is `Moving` and fires outside an
/// attack order, and an air target at (`target_x`, `target_y`), which flies over the shooter's
/// path so that nothing but the weapon could turn the hull.
fn free_fire_world(
    target_x: i32,
    target_y: i32,
) -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
    let weapon = r#"Some((
            name: "Free Rifle",
            visual_kind: Hitscan,
            target_flags: (bits: 255),
            min_range: 0.0,
            max_range: 6.0,
            firing_arc_deg: 30.0,
            acquire_arc_deg: 30.0,
            has_turret: false,
            turret_turn_rate_deg_per_tick: 0.0,
            turret_idle: Hold,
            allowed_movement: Moving,
            fires_outside_attack_order: true,
            base_damage: 1.0,
            attribute_bonuses: [],
            attacks_count: 1,
            strike_interval_ticks: 0,
            period_ticks: 20,
            damage_point_ticks: 2,
            backswing_ticks: 0,
            random_delay_ticks: (0.0, 0.0),
            splash: None,
            projectile_speed: None,
        ))"#;
    let shooter = unit_ron(SHOOTER, 1, 20.0, 0.2, false, None, weapon)
        .replace("            weapon: ", "            air_weapon: ");
    let target = unit_ron(TARGET, 2, 180.0, 0.1, false, None, "None")
        .replace("mover_type: Ground", "mover_type: Air");
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 7);
    world.catalog = UnitCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        &format!("(units: [{shooter}, {target}])"),
    )?;
    let at = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    let shooter_type = world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
    let target_type = world.catalog.id_by_slug(TARGET).ok_or("target")?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), at(10, 16))?;
    let target = world.spawn_unit(target_type, PlayerId(1), at(target_x, target_y))?;
    Ok((world, shooter, target))
}

/// P-4 (plan `2026-11-23-turret-wind-up` D-5): a hull weapon firing outside an attack order never
/// turns its unit's hull. Under a move east, the shooter's heading equals, tick by tick, its
/// heading in a world whose target is out of reach; it fires at a target in its arc and not at
/// one behind it.
#[test]
fn a_hull_weapon_outside_an_attack_order_never_turns_the_hull() -> TestResult {
    for (x, y, in_arc) in [(15, 18, true), (5, 16, false)] {
        let (mut world, shooter, target) = free_fire_world(x, y)?;
        let (mut reference, reference_shooter, _) = free_fire_world(40, 40)?;
        let mut fired = false;
        for tick in 0..40_u32 {
            let commands = |unit| {
                if tick > 0 {
                    return Vec::new();
                }
                vec![PlayerCommand {
                    player_id: PlayerId(0),
                    entities: vec![unit],
                    order: UnitOrder::Move {
                        target: SimVec2::new(SimScalar::from_i32(40), SimScalar::from_i32(16)),
                    },
                    queue: false,
                }]
            };
            for (world, unit) in [(&mut world, shooter), (&mut reference, reference_shooter)] {
                world.step(&TickBundle {
                    tick: Tick(tick),
                    commands: commands(unit),
                    state_checksum: None,
                });
            }
            let unit = world.entities.get(&shooter).ok_or("shooter")?;
            let heading = reference
                .entities
                .get(&reference_shooter)
                .ok_or("reference shooter")?
                .heading;
            assert_eq!(
                unit.heading, heading,
                "target at ({x}, {y}), tick {tick}: the weapon never turns the hull"
            );
            fired |= !matches!(
                unit.weapon_phase,
                WeaponPhase::Ready | WeaponPhase::RotatingToTarget { .. }
            );
        }
        let hp = world.entities.get(&target).ok_or("target")?.hp;
        assert_eq!(
            fired, in_arc,
            "target at ({x}, {y}): fires only at a target in its arc"
        );
        assert_eq!(
            hp < SimScalar::from_i32(10_000),
            in_arc,
            "target at ({x}, {y}): damage"
        );
    }
    Ok(())
}

/// How the shooter of [`hull_between_shots`] stands, and why its weapon works.
// Each flag is one independent condition of the case table, so a test reads as its table row.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
struct BetweenShots {
    /// The phase between shots: `Backswing` (with the cooldown running) or `Cooldown`.
    backswing: bool,
    /// The unit is on Hold Position.
    hold: bool,
    /// The unit is immobile.
    immobile: bool,
    /// Why the weapon works against its target.
    mandate: Mandate,
    /// What is left of this tick's hull budget, binary units.
    budget: u16,
    /// The weapon locks its turret while firing (a hull weapon has none to lock).
    locks: bool,
    /// The weapon has a target this tick, due south of the shooter.
    target: bool,
}

/// One direct weapon step of the scratch shooter (hull weapon, facing +x, 7 units north of its
/// target) between shots, as `case` says; returns its heading after the step.
fn hull_between_shots(case: BetweenShots) -> Result<SimAngle, Box<dyn std::error::Error>> {
    let shooter = Shooter {
        turn_rate: 20.16,
        max_speed: 0.15,
        arc: 15.0,
        turret: None,
        turn_before_move: false,
        stationary: Some(STATIONARY),
    };
    let bench = Bench::new(shooter, 10, 23)?;
    let shooter_type = bench.world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
    let mut weapon = bench
        .world
        .catalog
        .get(shooter_type)?
        .weapon
        .clone()
        .ok_or("the shooter's weapon")?;
    weapon.locks_turret_while_firing = case.locks;
    let mut entity = bench
        .world
        .entities
        .get(&bench.shooter)
        .ok_or("shooter")?
        .clone();
    entity.heading = SimAngle::ZERO;
    entity.weapon_cooldown_remaining = 5;
    entity.weapon_phase = if case.backswing {
        WeaponPhase::Backswing {
            target_id: bench.target,
            remaining_ticks: 3,
        }
    } else {
        WeaponPhase::Cooldown
    };
    entity.order_queue.clear();
    if case.hold {
        entity.issue_order(UnitOrder::HoldPosition, false)?;
    }
    entity.is_immobile = case.immobile;
    entity.hull_turn_budget = case.budget;
    let target = case.target.then(|| WeaponTarget {
        id: bench.target,
        position: point(10, 23),
        combined_radius: SimScalar::ONE,
    });
    assert!(
        !step_entity_weapon(&mut entity, &weapon, target, case.mandate),
        "nothing is released between shots"
    );
    Ok(entity.heading)
}

/// The facing due south of the shooter of [`hull_between_shots`].
fn south() -> SimAngle {
    facing(point(10, 16), point(10, 23))
}

/// The holding case of [`hull_between_shots`]: a full stationary budget, a target, an attack
/// order's mandate, no lock, in a cooldown.
const HOLDING: BetweenShots = BetweenShots {
    backswing: false,
    hold: true,
    immobile: false,
    mandate: Mandate::AttackOrder,
    budget: STATIONARY_UNITS,
    locks: false,
    target: true,
};

/// B-1 (direct) of plan `2026-11-25-wind-up-and-hold-facing` (D-4): in its backswing and in its
/// cooldown, a holding unit's hull weapon turns the hull towards its target by the tick's
/// budget, the stationary rate.
#[test]
fn a_holding_hull_turns_towards_its_target_between_shots() -> TestResult {
    for backswing in [false, true] {
        let after = hull_between_shots(BetweenShots {
            backswing,
            ..HOLDING
        })?;
        assert_eq!(
            after,
            SimAngle::ZERO.rotate_towards(south(), STATIONARY_UNITS),
            "backswing {backswing}"
        );
    }
    Ok(())
}

/// B-2 of plan `2026-11-25-wind-up-and-hold-facing` (D-4): a mobile unit that does not hold,
/// whose hull pursuit aims under its attack order, is not turned by its weapon between shots.
#[test]
fn the_weapon_does_not_turn_a_hull_pursuit_aims() -> TestResult {
    for backswing in [false, true] {
        let after = hull_between_shots(BetweenShots {
            backswing,
            hold: false,
            ..HOLDING
        })?;
        assert_eq!(after, SimAngle::ZERO, "backswing {backswing}");
    }
    Ok(())
}

/// B-3 of plan `2026-11-25-wind-up-and-hold-facing` (D-4; turret wind-up D-5): a hull weapon
/// working on its own initiative never turns the hull, holding or not.
#[test]
fn a_weapon_on_its_own_initiative_never_turns_the_hull_between_shots() -> TestResult {
    for hold in [false, true] {
        let after = hull_between_shots(BetweenShots {
            hold,
            mandate: Mandate::OwnInitiative,
            ..HOLDING
        })?;
        assert_eq!(after, SimAngle::ZERO, "hold {hold}");
    }
    Ok(())
}

/// B-4 of plan `2026-11-25-wind-up-and-hold-facing` (D-4): an immobile unit's hull weapon turns
/// the hull by what the tick leaves it; with no budget (a structure's stationary rate of 0) the
/// hull never turns.
#[test]
fn an_immobile_hull_turns_by_its_budget_and_not_at_all_without_one() -> TestResult {
    let immobile = BetweenShots {
        hold: false,
        immobile: true,
        ..HOLDING
    };
    assert_eq!(
        hull_between_shots(BetweenShots {
            budget: 0,
            ..immobile
        })?,
        SimAngle::ZERO
    );
    assert_eq!(
        hull_between_shots(immobile)?,
        SimAngle::ZERO.rotate_towards(south(), STATIONARY_UNITS)
    );
    Ok(())
}

/// B-5 of plan `2026-11-25-wind-up-and-hold-facing` (D-4; fire-rules D-2): a turret lock never
/// holds a hull: a hull weapon flagged to lock its turret still turns the hull in its backswing.
/// Catalog validation is not asked; the flag is written on the step's weapon.
#[test]
fn a_turret_lock_never_holds_a_hull() -> TestResult {
    let after = hull_between_shots(BetweenShots {
        backswing: true,
        locks: true,
        ..HOLDING
    })?;
    assert_eq!(
        after,
        SimAngle::ZERO.rotate_towards(south(), STATIONARY_UNITS)
    );
    Ok(())
}

/// B-6 of plan `2026-11-25-wind-up-and-hold-facing` (D-4): with no target this tick a holding
/// unit keeps its heading between shots.
#[test]
fn with_no_target_a_holding_hull_keeps_its_heading() -> TestResult {
    for backswing in [false, true] {
        let after = hull_between_shots(BetweenShots {
            backswing,
            target: false,
            ..HOLDING
        })?;
        assert_eq!(after, SimAngle::ZERO, "backswing {backswing}");
    }
    Ok(())
}

/// B-1 of plan `2026-11-25-wind-up-and-hold-facing` (D-4, B-199): a holding unit whose hull
/// turns slowly (3° a tick standing, a 5° arc) fires at a target that walks across its front
/// without ever turning to it after its first swing: its hull follows the target through every
/// cooldown, so each next swing starts on the tick the cooldown allows. A hull left where it
/// fired would rotate to the target after each cooldown first.
#[test]
fn a_slow_holding_hull_never_waits_to_turn_after_its_first_swing() -> TestResult {
    let shooter = Shooter {
        turn_rate: 3.0,
        max_speed: 0.15,
        arc: 5.0,
        turret: None,
        turn_before_move: false,
        stationary: Some(3.0),
    };
    let mut bench = Bench::new(shooter, 10, 20)?;
    let target = bench.target;
    bench
        .world
        .entities
        .get_mut(&bench.shooter)
        .ok_or("shooter")?
        .heading = south();
    bench.step(Some((bench.shooter, UnitOrder::HoldPosition)));
    bench.step(Some((
        target,
        UnitOrder::Move {
            target: point(40, 20),
        },
    )));
    let mut swings = 0;
    let mut previous = bench.phase()?;
    for tick in 3..=60_u32 {
        bench.step(None);
        let phase = bench.phase()?;
        if matches!(phase, WeaponPhase::Windup { .. })
            && !matches!(previous, WeaponPhase::Windup { .. })
        {
            swings += 1;
        }
        if swings > 0 {
            assert!(
                !matches!(phase, WeaponPhase::RotatingToTarget { .. }),
                "tick {tick}: the hull already faces its target"
            );
        }
        previous = phase;
    }
    assert!(
        swings >= 2,
        "premise: the unit fired more than once, {swings}"
    );
    Ok(())
}
