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
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, Tick, TickBundle};

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
        }
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
            base_damage: {STRIKE:.1},
            attribute_bonuses: [],
            attacks_count: {strikes},
            strike_interval_ticks: {interval},
            {max_distance}
            period_ticks: {period},
            damage_point_ticks: 0,
            backswing_ticks: 0,
            splash: None,
            projectile_speed: {speed},
        ))"#,
        strikes = burst.strikes,
        interval = burst.interval,
        period = burst.period,
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

/// A shooter at (10, 16) and a target `distance` world units to its right.
struct Bench {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
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
        Ok(Self {
            world,
            shooter,
            target,
            tick: 0,
        })
    }

    fn step(&mut self) {
        self.world.step(&TickBundle {
            tick: Tick(self.tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        self.tick += 1;
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
