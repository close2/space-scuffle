//! Behavioural tests for the timing of multi-strike weapons (plan
//! `2026-11-05-sc2-deviation-fixes` WP5, B-2).
//!
//! # Responsibility
//! Pins the rule that the strikes of one attack land on the ticks the weapon's
//! `strike_interval_ticks` names: the first on the release tick, each later one an interval
//! after the one before. In `StarCraft` II a multi-strike weapon is a `CEffectCreatePersistent`
//! whose `PeriodicPeriodArray` spaces its strikes (the Zealot's `PsiBladesBurst`: 0 and 0.28
//! Normal s, `liberty` EffectData:1919), and a strike's own validator can drop it (the Zealot's
//! second blade lands only within 2.1, `voidmulti` ValidatorData:867). An interval of 0 keeps
//! every strike on the release tick, as before WP5.
//!
//! It also pins the first strike's delay (plan `2026-11-23-strike-delays`, B-81): the first
//! entry of the `PeriodicPeriodArray` comes before the first strike too (the Liberator's
//! `LiberatorMissileBurstPersistent`, 0.05 Normal s, `void` EffectData:2942-2970), so a weapon
//! with `first_strike_delay_ticks` lands every strike of a burst that many ticks later, and each
//! strike, the first included, is validated when it falls due.
//!
//! # How it fits
//! The tests drive the public world API (`SimWorld::step`) on a flat map and read the target's
//! life and the projectiles in flight between ticks, as a battle sees them. Catalog validation
//! of the new fields is checked through `UnitCatalog::from_ron_str`.
//!
//! # Invariants
//! Every archetype is a scratch archetype written in this file, never a shipped identity (C5).
//! Ticks are 25 Hz simulation ticks; distances are world units, centre to centre.

use sim_core::catalog::UnitCatalog;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The scratch shooter's slug.
const SHOOTER: &str = "burst_shooter";
/// The scratch target's slug.
const TARGET: &str = "burst_target";
/// Damage of one strike of the scratch weapon.
const STRIKE: f32 = 10.0;
/// Life of the scratch target, far more than any test deals.
const TARGET_LIFE: f32 = 1000.0;

/// How the scratch shooter's two-strike weapon spaces its strikes.
#[derive(Debug, Clone, Copy)]
struct Burst {
    /// `strike_interval_ticks` of the weapon.
    interval: u16,
    /// `followup_max_distance` of the weapon, if any.
    max_distance: Option<f32>,
    /// `projectile_speed` of the weapon, world units per tick, or `None` for a hitscan weapon.
    projectile_speed: Option<f32>,
    /// `attacks_count` of the weapon.
    strikes: u8,
    /// `period_ticks` of the weapon.
    period: u16,
    /// `channeled` of the weapon: an explicit order between the strikes ends the burst.
    channeled: bool,
    /// `first_strike_delay_ticks` of the weapon: ticks from the release to the first strike.
    delay: u16,
}

impl Burst {
    /// A hitscan weapon of two strikes spaced `interval` ticks apart, period 40.
    const fn hitscan(interval: u16) -> Self {
        Self {
            interval,
            max_distance: None,
            projectile_speed: None,
            strikes: 2,
            period: 40,
            channeled: false,
            delay: 0,
        }
    }

    /// This burst with its first strike `delay` ticks after the release.
    const fn delayed(self, delay: u16) -> Self {
        Self { delay, ..self }
    }
}

/// The scratch weapon in the catalog's RON form.
fn weapon_ron(burst: Burst) -> String {
    let max_distance = burst.max_distance.map_or(String::new(), |distance| {
        format!("followup_max_distance: Some({distance:.2}),")
    });
    let (visual, speed) = burst.projectile_speed.map_or_else(
        || ("Hitscan".to_owned(), "None".to_owned()),
        |speed| ("Projectile".to_owned(), format!("Some({speed:.2})")),
    );
    format!(
        r#"Some((
            name: "Burst Rifle",
            visual_kind: {visual},
            target_flags: (bits: 255),
            min_range: 0.0,
            max_range: 6.0,
            firing_arc_deg: 360.0,
            acquire_arc_deg: 360.0,
            has_turret: false,
            turret_turn_rate_deg_per_tick: 0.0,
            turret_idle: Hold,
            base_damage: {STRIKE:.1},
            attribute_bonuses: [],
            attacks_count: {strikes},
            first_strike_delay_ticks: {delay},
            strike_interval_ticks: {interval},
            {max_distance}
            channeled: {channeled},
            period_ticks: {period},
            damage_point_ticks: 0,
            backswing_ticks: 0,
            random_delay_ticks: (0.0, 0.0),
            splash: None,
            projectile_speed: {speed},
        ))"#,
        strikes = burst.strikes,
        interval = burst.interval,
        delay = burst.delay,
        period = burst.period,
        channeled = burst.channeled,
    )
}

/// One scratch unit in the catalog's RON form: immobile, unarmoured, radius 0.5.
fn unit_ron(slug: &str, id: u16, life: f32, weapon: &str) -> String {
    format!(
        r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of multi_strike_timing_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: Ground,
            attributes: (bits: 1),
            max_hp: {life:.1},
            max_shields: 0.0,
            max_energy: 0.0,
            start_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: 0.0,
            acceleration: 0.0,
            deceleration: 0.0,
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
            weapon: {weapon},
            is_detector: false,
        )"#
    )
}

/// The scratch catalog: the shooter with `burst`'s weapon and an unarmed target.
fn catalog_ron(burst: Burst) -> String {
    format!(
        "(units: [{}, {}])",
        unit_ron(SHOOTER, 1, TARGET_LIFE, &weapon_ron(burst)),
        unit_ron(TARGET, 2, TARGET_LIFE, "None"),
    )
}

/// A point 6 world units to the shooter's left, away from both enemies.
fn away() -> SimVec2 {
    SimVec2::new(SimScalar::from_i32(4), SimScalar::from_i32(16))
}

/// A shooter at (10, 16), a target `distance` world units to its right and a bystander of the
/// target's player 5 world units below the shooter.
struct Bench {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
    bystander: EntityId,
    tick: u32,
}

impl Bench {
    fn new(burst: Burst, distance: f32) -> Result<Self, Box<dyn std::error::Error>> {
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = sim_fixtures::world(map, 7);
        world.catalog =
            UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &catalog_ron(burst))?;
        let shooter_type = world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
        let target_type = world.catalog.id_by_slug(TARGET).ok_or("target")?;
        let x = SimScalar::from_i32(10);
        let y = SimScalar::from_i32(16);
        let shooter = world.spawn_unit(shooter_type, PlayerId(0), SimVec2::new(x, y))?;
        let offset = SimScalar::from_f32_lossy(distance);
        let target = world.spawn_unit(target_type, PlayerId(1), SimVec2::new(x + offset, y))?;
        let below = SimVec2::new(x, y + SimScalar::from_i32(5));
        let bystander = world.spawn_unit(target_type, PlayerId(1), below)?;
        Ok(Self {
            world,
            shooter,
            target,
            bystander,
            tick: 0,
        })
    }

    fn step(&mut self) {
        self.step_with(Vec::new());
    }

    /// Steps one tick with `commands`.
    fn step_with(&mut self, commands: Vec<PlayerCommand>) {
        self.world.step(&TickBundle {
            tick: Tick(self.tick),
            commands,
            state_checksum: None,
        });
        self.tick += 1;
    }

    /// The shooter's player gives it `order`, queued or not.
    fn order(&self, order: UnitOrder, queue: bool) -> PlayerCommand {
        PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![self.shooter],
            order,
            queue,
        }
    }

    /// Life the target has lost so far.
    fn damage_taken(&self) -> Result<f32, Box<dyn std::error::Error>> {
        let target = self.world.entities.get(&self.target).ok_or("target")?;
        Ok(TARGET_LIFE - target.hp.to_f32())
    }

    /// Steps `ticks` ticks and returns the target's damage taken after each one.
    fn damage_trace(&mut self, ticks: u32) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let mut trace = Vec::new();
        for _ in 0..ticks {
            self.step();
            trace.push(self.damage_taken()?);
        }
        Ok(trace)
    }
}

/// The index of the first tick of `trace` on which damage was taken.
fn first_hit(trace: &[f32]) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(trace
        .iter()
        .position(|&damage| damage > 0.0)
        .ok_or("the shooter never hit")?)
}

#[test]
fn strikes_with_no_interval_land_on_the_release_tick() -> TestResult {
    let mut bench = Bench::new(Burst::hitscan(0), 3.0)?;
    let trace = bench.damage_trace(20)?;
    let release = first_hit(&trace)?;
    assert!(
        (trace[release] - 2.0 * STRIKE).abs() < 1e-3,
        "both strikes on the release tick: {trace:?}"
    );
    Ok(())
}

#[test]
fn the_second_strike_lands_one_interval_after_the_first() -> TestResult {
    let mut bench = Bench::new(Burst::hitscan(5), 3.0)?;
    let trace = bench.damage_trace(30)?;
    let release = first_hit(&trace)?;
    for (offset, &damage) in trace[release..release + 5].iter().enumerate() {
        assert!(
            (damage - STRIKE).abs() < 1e-3,
            "one strike for the first 5 ticks (offset {offset}): {trace:?}"
        );
    }
    assert!(
        (trace[release + 5] - 2.0 * STRIKE).abs() < 1e-3,
        "the second strike 5 ticks after the first: {trace:?}"
    );
    assert!(
        (trace[release + 10] - 2.0 * STRIKE).abs() < 1e-3,
        "no third strike: {trace:?}"
    );
    Ok(())
}

#[test]
fn a_followup_beyond_its_distance_is_lost() -> TestResult {
    let burst = Burst {
        max_distance: Some(2.1),
        ..Burst::hitscan(5)
    };
    let mut far = Bench::new(burst, 3.0)?;
    let trace = far.damage_trace(30)?;
    let release = first_hit(&trace)?;
    assert!(
        (trace[release + 10] - STRIKE).abs() < 1e-3,
        "the second strike is lost beyond 2.1: {trace:?}"
    );

    let mut near = Bench::new(burst, 2.0)?;
    let trace = near.damage_trace(30)?;
    let release = first_hit(&trace)?;
    assert!(
        (trace[release + 5] - 2.0 * STRIKE).abs() < 1e-3,
        "the second strike lands within 2.1: {trace:?}"
    );
    Ok(())
}

#[test]
fn a_dead_attacker_lands_no_further_strike() -> TestResult {
    let mut bench = Bench::new(Burst::hitscan(5), 3.0)?;
    let mut released = false;
    for _ in 0..30 {
        bench.step();
        if !released && bench.damage_taken()? > 0.0 {
            released = true;
            let shooter = bench
                .world
                .entities
                .get_mut(&bench.shooter)
                .ok_or("shooter")?;
            shooter.hp = SimScalar::ZERO;
        }
    }
    assert!(released, "the shooter released an attack");
    assert!(
        (bench.damage_taken()? - STRIKE).abs() < 1e-3,
        "only the strike released before the shooter died"
    );
    Ok(())
}

#[test]
fn a_spaced_projectile_weapon_launches_one_missile_per_strike() -> TestResult {
    let burst = Burst {
        projectile_speed: Some(0.25),
        ..Burst::hitscan(3)
    };
    let mut bench = Bench::new(burst, 4.0)?;
    let mut in_flight = Vec::new();
    for _ in 0..30 {
        bench.step();
        in_flight.push(bench.world.projectiles.len());
    }
    let launch = in_flight
        .iter()
        .position(|&count| count > 0)
        .ok_or("no projectile launched")?;
    assert_eq!(
        &in_flight[launch..launch + 4],
        &[1, 1, 1, 2],
        "one missile on the release tick, the second 3 ticks later: {in_flight:?}"
    );
    let trace = bench.damage_taken()?;
    assert!(
        (trace - 2.0 * STRIKE).abs() < 1e-3,
        "each missile lands one strike: {trace}"
    );
    Ok(())
}

/// Damage the target has taken 10 ticks after the shooter's first strike, when the shooter is
/// given `order(bench)` (queued if `queue`) on the tick after that strike, between the strikes of
/// a 5-tick burst.
fn damage_after_order_between_strikes(
    channeled: bool,
    queue: bool,
    order: impl Fn(&Bench) -> UnitOrder,
) -> Result<f32, Box<dyn std::error::Error>> {
    let burst = Burst {
        channeled,
        ..Burst::hitscan(5)
    };
    let mut bench = Bench::new(burst, 3.0)?;
    // A shooter that cannot walk takes no Hold Position (plan 2026-11-23-stance-checked-orders
    // D-1, D-2), so the bench's shooter is given a speed here: every order of
    // `overriding_orders` then reaches it, and it stands still because nothing tells it to walk
    // before the order under test.
    let shooter_type = bench.world.entities[&bench.shooter].unit_type_id;
    bench.world.catalog.get_mut(shooter_type)?.max_speed = SimScalar::from_num(0.1);
    for _ in 0..30 {
        bench.step();
        if bench.damage_taken()? > 0.0 {
            break;
        }
    }
    assert!(
        (bench.damage_taken()? - STRIKE).abs() < 1e-3,
        "the first strike landed"
    );
    let command = bench.order(order(&bench), queue);
    bench.step_with(vec![command]);
    for _ in 0..9 {
        bench.step();
    }
    bench.damage_taken()
}

/// A named order, built from the bench it is given in.
type NamedOrder = (&'static str, fn(&Bench) -> UnitOrder);

/// The orders Q0058 names, each replacing the attacker's orders between the strikes.
fn overriding_orders() -> Vec<NamedOrder> {
    vec![
        ("Move", |_| UnitOrder::Move { target: away() }),
        ("Stop", |_| UnitOrder::Stop),
        ("HoldPosition", |_| UnitOrder::HoldPosition),
        ("AttackMove", |_| UnitOrder::AttackMove { target: away() }),
        ("AttackTarget on another unit", |bench| {
            UnitOrder::attack(bench.bystander)
        }),
    ]
}

#[test]
fn q0058_an_explicit_order_ends_a_channeled_burst() -> TestResult {
    for (name, order) in overriding_orders() {
        let damage = damage_after_order_between_strikes(true, false, order)?;
        assert!(
            (damage - STRIKE).abs() < 1e-3,
            "{name} between the strikes ends a channeled burst: damage {damage}"
        );
    }
    Ok(())
}

#[test]
fn q0058_a_channeled_burst_survives_a_queued_order_and_its_own_target() -> TestResult {
    let damage =
        damage_after_order_between_strikes(true, true, |_| UnitOrder::Move { target: away() })?;
    assert!(
        (damage - 2.0 * STRIKE).abs() < 1e-3,
        "a queued order does not replace the attack: damage {damage}"
    );
    let damage =
        damage_after_order_between_strikes(true, false, |bench| UnitOrder::attack(bench.target))?;
    assert!(
        (damage - 2.0 * STRIKE).abs() < 1e-3,
        "an attack on the burst's own target keeps it: damage {damage}"
    );
    Ok(())
}

#[test]
fn q0058_an_order_does_not_end_a_burst_that_is_not_channeled() -> TestResult {
    for (name, order) in overriding_orders() {
        let damage = damage_after_order_between_strikes(false, false, order)?;
        assert!(
            (damage - 2.0 * STRIKE).abs() < 1e-3,
            "{name} between the strikes keeps a burst that is not channeled: damage {damage}"
        );
    }
    Ok(())
}

/// Asserts that the scratch catalog with `burst` fails to load, naming `field`.
fn assert_rejected(burst: Burst, field: &str) {
    let loaded = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &catalog_ron(burst));
    let error = loaded.err().map(|error| error.to_string());
    assert!(
        error.as_deref().is_some_and(|error| error.contains(field)),
        "{burst:?} must be rejected naming {field}; got {error:?}"
    );
}

#[test]
fn catalog_rejects_strike_spacing_it_cannot_honour() {
    // A single strike has no interval.
    assert_rejected(
        Burst {
            strikes: 1,
            ..Burst::hitscan(5)
        },
        "strike_interval_ticks",
    );
    // The last strike must land within the attack period: 2 strikes, 40 ticks apart.
    assert_rejected(Burst::hitscan(40), "strike_interval_ticks");
    // A distance condition belongs to strikes on later ticks.
    assert_rejected(
        Burst {
            max_distance: Some(2.1),
            ..Burst::hitscan(0)
        },
        "followup_max_distance",
    );
    assert_rejected(
        Burst {
            max_distance: Some(0.0),
            ..Burst::hitscan(5)
        },
        "followup_max_distance",
    );
}

#[test]
fn catalog_accepts_a_burst_that_ends_within_its_period() -> TestResult {
    let burst = Burst {
        strikes: 3,
        period: 21,
        max_distance: Some(2.1),
        ..Burst::hitscan(10)
    };
    UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &catalog_ron(burst))?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The first strike's delay (plan 2026-11-23-strike-delays, B-81). A burst persistent's first
// period comes before its first strike as well as between strikes (`StarCraft` II
// `PeriodicPeriodArray[0]`; the Liberator's `LiberatorMissileBurstPersistent`, `void`
// EffectData:2942-2970). `first_strike_delay_ticks` moves every strike of a burst that many ticks
// after the release, and each strike, the first included, is validated when it falls due.
// ---------------------------------------------------------------------------------------------

/// Ticks each delay test traces: more than two attack periods of 40.
const WINDOW: u32 = 100;

/// Ticks a test watches after a release: longer than any scratch burst with its delay.
const AFTER_RELEASE: usize = 20;

impl Bench {
    /// Steps `ticks` ticks and returns how many projectiles were launched on each.
    fn launch_trace(&mut self, ticks: u32) -> Vec<usize> {
        let mut seen: Vec<u32> = Vec::new();
        let mut trace = Vec::new();
        for _ in 0..ticks {
            self.step();
            let fresh: Vec<u32> = self
                .world
                .projectiles
                .iter()
                .map(|projectile| projectile.id)
                .filter(|id| !seen.contains(id))
                .collect();
            trace.push(fresh.len());
            seen.extend(fresh);
        }
        trace
    }

    /// Steps up to and including the tick of trace index `tick`; the next step is `tick + 1`.
    fn step_through(&mut self, tick: usize) -> Result<(), Box<dyn std::error::Error>> {
        while usize::try_from(self.tick)? <= tick {
            self.step();
        }
        Ok(())
    }

    /// Gives the shooter a walking speed, so that every order of [`overriding_orders`] reaches
    /// it (a unit that cannot walk takes no Hold Position). It stands still until an order tells
    /// it to walk.
    fn let_shooter_walk(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let shooter_type = self.world.entities[&self.shooter].unit_type_id;
        self.world.catalog.get_mut(shooter_type)?.max_speed = SimScalar::from_num(0.1);
        Ok(())
    }
}

/// The trace index of the tick on which `burst`'s shooter releases its first attack at a target
/// `distance` away. It is read from the same burst without a delay, whose first strike lands (or
/// first missile leaves) on its release tick; nothing before the release depends on the delay.
fn release_tick(burst: Burst, distance: f32) -> Result<usize, Box<dyn std::error::Error>> {
    let mut bench = Bench::new(burst.delayed(0), distance)?;
    if burst.projectile_speed.is_some() {
        let launches = bench.launch_trace(40);
        return Ok(launches
            .iter()
            .position(|&count| count > 0)
            .ok_or("the shooter never launched")?);
    }
    first_hit(&bench.damage_trace(40)?)
}

/// Asserts that the damage trace `delayed` is `undelayed` moved `delay` ticks later: no damage
/// before, then the same damage tick for tick.
fn assert_damage_shifted(undelayed: &[f32], delayed: &[f32], delay: u16, what: &str) {
    let delay = usize::from(delay);
    for (tick, &damage) in delayed.iter().enumerate() {
        let expected = tick
            .checked_sub(delay)
            .and_then(|earlier| undelayed.get(earlier))
            .copied()
            .unwrap_or_default();
        assert!(
            (damage - expected).abs() < 1e-3,
            "{what}, delay {delay}, tick {tick}: expected {expected}, got {damage}\n\
             undelayed {undelayed:?}\ndelayed {delayed:?}"
        );
    }
}

/// Asserts that the launch trace `delayed` is `undelayed` moved `delay` ticks later.
fn assert_launches_shifted(undelayed: &[usize], delayed: &[usize], delay: u16) {
    let delay = usize::from(delay);
    let expected: Vec<usize> = (0..delayed.len())
        .map(|tick| {
            tick.checked_sub(delay)
                .and_then(|earlier| undelayed.get(earlier))
                .copied()
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        delayed, expected,
        "launches with delay {delay}; undelayed {undelayed:?}"
    );
}

/// Asserts that `burst` with each of `delays` lands the damage of `burst` without a delay, each
/// strike that many ticks later, at a target 3 world units away.
fn assert_every_strike_shifted(burst: Burst, delays: &[u16], what: &str) -> TestResult {
    let undelayed = Bench::new(burst, 3.0)?.damage_trace(WINDOW)?;
    let landed = undelayed.last().copied().unwrap_or_default();
    assert!(
        landed >= 2.0 * f32::from(burst.strikes) * STRIKE - 1e-3,
        "premise: two whole attacks land in the window: {undelayed:?}"
    );
    for &delay in delays {
        let delayed = Bench::new(burst.delayed(delay), 3.0)?.damage_trace(WINDOW)?;
        assert_damage_shifted(&undelayed, &delayed, delay, what);
    }
    Ok(())
}

#[test]
fn a_delayed_burst_lands_every_strike_after_the_delay() -> TestResult {
    assert_every_strike_shifted(Burst::hitscan(5), &[1, 3], "two strikes 5 apart")?;
    assert_every_strike_shifted(
        Burst {
            strikes: 3,
            ..Burst::hitscan(4)
        },
        &[1, 3],
        "three strikes 4 apart",
    )
}

#[test]
fn a_delayed_volley_lands_whole_after_the_delay() -> TestResult {
    let volley = Burst::hitscan(0);
    assert_every_strike_shifted(volley, &[1, 3], "a volley of two")?;
    let release = release_tick(volley, 3.0)?;
    let trace = Bench::new(volley.delayed(3), 3.0)?.damage_trace(WINDOW)?;
    assert!(
        trace[release + 2].abs() < 1e-3 && (trace[release + 3] - 2.0 * STRIKE).abs() < 1e-3,
        "both strikes of the volley land together, 3 ticks after the release: {trace:?}"
    );
    Ok(())
}

#[test]
fn a_delayed_single_strike_lands_after_the_delay() -> TestResult {
    let single = Burst {
        strikes: 1,
        ..Burst::hitscan(0)
    };
    assert_every_strike_shifted(single, &[1, 3], "a single strike")
}

#[test]
fn a_delayed_projectile_burst_launches_from_where_its_attacker_is() -> TestResult {
    let burst = Burst {
        projectile_speed: Some(0.25),
        ..Burst::hitscan(3)
    };
    let undelayed = Bench::new(burst, 4.0)?.launch_trace(WINDOW);
    assert!(
        undelayed.iter().sum::<usize>() >= 4,
        "premise: at least two attacks of two missiles each: {undelayed:?}"
    );
    for delay in [1, 3] {
        let delayed = Bench::new(burst.delayed(delay), 4.0)?.launch_trace(WINDOW);
        assert_launches_shifted(&undelayed, &delayed, delay);
    }

    // The shooter is moved one world unit after its release; the first missile leaves from
    // where it is on the tick the missile falls due, not from where it released.
    let release = release_tick(burst, 4.0)?;
    let mut bench = Bench::new(burst.delayed(3), 4.0)?;
    bench.step_through(release)?;
    let shooter = bench
        .world
        .entities
        .get_mut(&bench.shooter)
        .ok_or("shooter")?;
    let released_from = shooter.position;
    shooter.position = released_from + SimVec2::new(SimScalar::ZERO, SimScalar::ONE);
    bench.step_through(release + 3)?;
    let here = bench.world.entities[&bench.shooter].position;
    assert_ne!(here, released_from, "premise: the shooter moved");
    let origins: Vec<SimVec2> = bench
        .world
        .projectiles
        .iter()
        .map(|projectile| projectile.source_pos)
        .collect();
    assert_eq!(
        origins,
        vec![here],
        "one missile, launched from the shooter's position on its launch tick"
    );
    Ok(())
}

#[test]
fn nothing_lands_on_the_release_tick_of_a_delayed_burst() -> TestResult {
    let hitscan = Burst::hitscan(5);
    let release = release_tick(hitscan, 3.0)?;
    let mut bench = Bench::new(hitscan.delayed(2), 3.0)?;
    bench.step_through(release)?;
    assert!(
        bench.damage_taken()?.abs() < 1e-3,
        "no strike on the release tick"
    );

    let projectile = Burst {
        projectile_speed: Some(0.25),
        ..Burst::hitscan(3)
    };
    let release = release_tick(projectile, 4.0)?;
    let mut bench = Bench::new(projectile.delayed(2), 4.0)?;
    bench.step_through(release)?;
    assert!(
        bench.world.projectiles.is_empty(),
        "no missile on the release tick"
    );
    Ok(())
}

/// Damage the target has taken `AFTER_RELEASE` ticks after `burst`'s first release, with the
/// target `distance` away, when `between(bench)` runs right after the release tick and
/// `after_first(bench)` right after the first strike's due tick.
fn damage_with_changes(
    burst: Burst,
    distance: f32,
    between: impl Fn(&mut Bench) -> TestResult,
    after_first: impl Fn(&mut Bench) -> TestResult,
) -> Result<f32, Box<dyn std::error::Error>> {
    let release = release_tick(burst, distance)?;
    let mut bench = Bench::new(burst, distance)?;
    bench.step_through(release)?;
    between(&mut bench)?;
    bench.step_through(release + usize::from(burst.delay))?;
    after_first(&mut bench)?;
    bench.step_through(release + AFTER_RELEASE)?;
    if bench.world.entities.contains_key(&bench.target) {
        bench.damage_taken()
    } else {
        Ok(TARGET_LIFE)
    }
}

#[test]
fn a_delayed_first_strike_is_validated_when_due() -> TestResult {
    let burst = Burst::hitscan(5).delayed(3);

    // The target is killed in the delay: no missile ever leaves for it.
    let missiles = Burst {
        projectile_speed: Some(0.25),
        ..Burst::hitscan(3)
    }
    .delayed(3);
    let release = release_tick(missiles, 4.0)?;
    let mut bench = Bench::new(missiles, 4.0)?;
    bench.step_through(release)?;
    bench
        .world
        .entities
        .get_mut(&bench.target)
        .ok_or("target")?
        .hp = SimScalar::ZERO;
    let launches = bench.launch_trace(u32::try_from(AFTER_RELEASE)?);
    assert_eq!(
        launches.iter().sum::<usize>(),
        0,
        "no missile for a target killed in the delay: {launches:?}"
    );

    // The target is cloaked, unrevealed, when the first strike falls due: that strike is lost,
    // and the second, after the target is seen again, still lands.
    let damage = damage_with_changes(
        burst,
        3.0,
        |bench| {
            let target = bench
                .world
                .entities
                .get_mut(&bench.target)
                .ok_or("target")?;
            target.is_cloaked = true;
            Ok(())
        },
        |bench| {
            let target = bench
                .world
                .entities
                .get_mut(&bench.target)
                .ok_or("target")?;
            target.is_cloaked = false;
            Ok(())
        },
    )?;
    assert!(
        (damage - STRIKE).abs() < 1e-3,
        "the first strike is lost on a cloaked target, the second lands: damage {damage}"
    );

    // A weapon with a distance condition checks it for the first strike too.
    let reach = Burst {
        max_distance: Some(2.1),
        ..burst
    };
    let far = damage_with_changes(reach, 3.0, |_| Ok(()), |_| Ok(()))?;
    assert!(far.abs() < 1e-3, "no strike beyond 2.1: damage {far}");
    let near = damage_with_changes(reach, 2.0, |_| Ok(()), |_| Ok(()))?;
    assert!(
        (near - 2.0 * STRIKE).abs() < 1e-3,
        "both strikes within 2.1: damage {near}"
    );

    // The attacker dies in the delay: no strike lands.
    let damage = damage_with_changes(
        burst,
        3.0,
        |bench| {
            let shooter = bench
                .world
                .entities
                .get_mut(&bench.shooter)
                .ok_or("shooter")?;
            shooter.hp = SimScalar::ZERO;
            Ok(())
        },
        |_| Ok(()),
    )?;
    assert!(
        damage.abs() < 1e-3,
        "a shooter dead in the delay lands nothing: damage {damage}"
    );
    Ok(())
}

/// Damage the target has taken `AFTER_RELEASE` ticks after the first release of a delayed burst
/// (two strikes 5 apart, the first 3 ticks after the release), when the shooter is given
/// `order(bench)` (queued if `queue`) on the tick after the release, inside the delay.
fn damage_after_order_in_the_delay(
    channeled: bool,
    queue: bool,
    order: impl Fn(&Bench) -> UnitOrder,
) -> Result<f32, Box<dyn std::error::Error>> {
    let burst = Burst {
        channeled,
        ..Burst::hitscan(5)
    }
    .delayed(3);
    let release = release_tick(burst, 3.0)?;
    let mut bench = Bench::new(burst, 3.0)?;
    bench.let_shooter_walk()?;
    bench.step_through(release)?;
    assert!(
        bench.damage_taken()?.abs() < 1e-3,
        "premise: nothing has landed yet"
    );
    let command = bench.order(order(&bench), queue);
    bench.step_with(vec![command]);
    bench.step_through(release + AFTER_RELEASE)?;
    bench.damage_taken()
}

#[test]
fn an_order_in_the_delay_ends_a_channeled_burst() -> TestResult {
    for (name, order) in overriding_orders() {
        let damage = damage_after_order_in_the_delay(true, false, order)?;
        assert!(
            damage.abs() < 1e-3,
            "{name} in the delay ends a channeled burst before its first strike: damage {damage}"
        );
    }
    let damage =
        damage_after_order_in_the_delay(true, false, |bench| UnitOrder::attack(bench.target))?;
    assert!(
        (damage - 2.0 * STRIKE).abs() < 1e-3,
        "an attack on the burst's own target keeps it: damage {damage}"
    );
    let damage =
        damage_after_order_in_the_delay(true, true, |_| UnitOrder::Move { target: away() })?;
    assert!(
        (damage - 2.0 * STRIKE).abs() < 1e-3,
        "a queued order does not replace the attack: damage {damage}"
    );
    Ok(())
}

#[test]
fn a_delayed_burst_that_is_not_channeled_survives_an_order() -> TestResult {
    for (name, order) in overriding_orders() {
        let damage = damage_after_order_in_the_delay(false, false, order)?;
        assert!(
            (damage - 2.0 * STRIKE).abs() < 1e-3,
            "{name} in the delay keeps a burst that is not channeled: damage {damage}"
        );
    }
    Ok(())
}
