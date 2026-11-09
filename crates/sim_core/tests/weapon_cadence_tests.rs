//! Behavioural tests for the weapon cadence: how often a weapon releases, whatever its backswing.
//!
//! # Responsibility
//! Pins the rule of plan `2026-11-04-combat-sc2-alignment` D-2 (Q0038): a backswing is a lock
//! on orders, not a gate on the weapon. When the cooldown runs out during a backswing, the
//! weapon acts as a ready weapon would; a new swing ends the backswing, and with nothing to
//! attack the backswing runs on to its end. It pins D-3 (Q0036): a damage point of 0 releases on
//! the tick its swing starts, so `period_ticks` is the attack cycle for every damage point. It
//! also pins the catalog rule that a weapon's period is longer than its damage point, and the
//! two arcs of plan `2026-11-05-sc2-deviation-fixes` WP10 (B-29): a weapon starts on a target it
//! is not engaged with only inside its acquisition arc (`Arc` ÷ 2), and keeps firing at the
//! target it is engaged with inside its engaged arc (`Arc` ÷ 2 + `ArcSlop`).
//!
//! # How it fits
//! `sim_core::combat::step_entity_weapon` runs once per unit per tick. Most tests drive it
//! through the public world API (`SimWorld::step`) and read which ticks a shot lands on, so
//! they see exactly what a battle sees. The tests of a single call (does the call that starts a
//! swing also release it) call `step_entity_weapon` directly on a copy of the spawned shooter.
//!
//! # Invariants
//! Every archetype here is a scratch archetype written in this file (a stationary shooter and
//! a stationary target), never a shipped identity, so a rebalance of the shipped catalog cannot
//! break these tests. Ticks are simulation ticks (25 per Faster second).

use sim_core::catalog::UnitCatalog;
use sim_core::{
    is_within_firing_arc, step_entity_weapon, Entity, SimCoreError, SimWorld, WeaponPhase,
    WeaponSpec, WeaponTarget,
};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The scratch shooter's slug.
const SHOOTER: &str = "cadence_shooter";
/// The scratch target's slug.
const TARGET: &str = "cadence_target";

/// The timings of the scratch shooter's weapon, in ticks.
#[derive(Debug, Clone, Copy)]
struct Timings {
    period: u16,
    damage_point: u16,
    backswing: u16,
}

/// One scratch unit definition in the catalog's RON form.
///
/// `weapon` is the RON of the `weapon` field and `overclock` of `capabilities.overclock`.
fn unit_ron(slug: &str, id: u16, max_hp: f32, weapon: &str, overclock: &str) -> String {
    format!(
        r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of weapon_cadence_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: Ground,
            attributes: (bits: 1),
            max_hp: {max_hp:.1},
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
            sight_radius: 9.0,
            scan_range: 9.0,
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
            capabilities: (overclock: {overclock}),
        )"#
    )
}

/// The RON of a hull-mounted weapon that may fire in every direction.
const OPEN_MOUNT: &str = "firing_arc_deg: 360.0, acquire_arc_deg: 360.0, has_turret: false, \
     turret_turn_rate_deg_per_tick: 0.0";

/// The scratch catalog: a shooter with `timings` and an optional overclock, and a target with
/// `target_hp` hit points. The shooter deals 10 damage per shot from up to 6 world units.
fn catalog_ron(timings: Timings, target_hp: f32, overclock: Option<f32>) -> String {
    catalog_ron_with_mount(timings, OPEN_MOUNT, target_hp, overclock)
}

/// As [`catalog_ron`], with `mount` the RON of the weapon's `firing_arc_deg`, `acquire_arc_deg`,
/// `has_turret` and `turret_turn_rate_deg_per_tick` fields.
fn catalog_ron_with_mount(
    timings: Timings,
    mount: &str,
    target_hp: f32,
    overclock: Option<f32>,
) -> String {
    let Timings {
        period,
        damage_point,
        backswing,
    } = timings;
    let weapon = format!(
        r#"Some((
            name: "Cadence Rifle",
            visual_kind: Hitscan,
            target_flags: (bits: 255),
            min_range: 0.0,
            max_range: 6.0,
            {mount},
            base_damage: 10.0,
            attribute_bonuses: [],
            attacks_count: 1,
            strike_interval_ticks: 0,
            period_ticks: {period},
            damage_point_ticks: {damage_point},
            backswing_ticks: {backswing},
            splash: None,
            projectile_speed: None,
        ))"#
    );
    let overclock = overclock.map_or_else(
        || "None".to_owned(),
        |rate| {
            format!(
                "Some((hp_cost: 0.0, duration_ticks: 5000, speed_multiplier: 1.0, \
                 attack_rate_multiplier: {rate:.2}))"
            )
        },
    );
    format!(
        "(units: [{}, {}])",
        unit_ron(SHOOTER, 1, 100.0, &weapon, &overclock),
        unit_ron(TARGET, 2, target_hp, "None", "None"),
    )
}

/// A shooter and a target three world units apart, the shooter ordered to attack the target.
struct Bench {
    world: SimWorld,
    shooter: EntityId,
    target: EntityId,
}

impl Bench {
    /// Builds the bench; with `stim`, applies the shooter's overclock before the first tick.
    fn new(
        timings: Timings,
        target_hp: f32,
        stim: Option<f32>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let Self {
            mut world,
            shooter,
            target,
        } = Self::spawned(&catalog_ron(timings, target_hp, stim), 13)?;
        let shooter_type = world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
        if stim.is_some() {
            let spec = world
                .catalog
                .get(shooter_type)?
                .overclock
                .ok_or("the scratch shooter has an overclock")?;
            let applied = world
                .entities
                .get_mut(&shooter)
                .ok_or("shooter missing")?
                .apply_overclock(spec)?;
            assert!(applied, "the overclock must be admitted");
        }
        world.step(&TickBundle {
            tick: Tick(0),
            commands: vec![PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![shooter],
                order: UnitOrder::attack(target),
                queue: false,
            }],
            state_checksum: None,
        });
        Ok(Self {
            world,
            shooter,
            target,
        })
    }

    /// The shooter at x = 10 and the target at x = `target_x` (both at y = 16, world units) of
    /// a flat map with the scratch catalog `catalog`. No tick has run and no order is given.
    fn spawned(catalog: &str, target_x: i32) -> Result<Self, Box<dyn std::error::Error>> {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = sim_fixtures::world(map, 11);
        world.catalog = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), catalog)?;
        let shooter_type = world.catalog.id_by_slug(SHOOTER).ok_or("shooter")?;
        let target_type = world.catalog.id_by_slug(TARGET).ok_or("target")?;
        let at = |x: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(16));
        let shooter = world.spawn_unit(shooter_type, PlayerId(0), at(10))?;
        let target = world.spawn_unit(target_type, PlayerId(1), at(target_x))?;
        Ok(Self {
            world,
            shooter,
            target,
        })
    }

    /// A copy of the spawned shooter, its weapon, and the target as the combat phase would
    /// pass it, for calling `step_entity_weapon` directly.
    fn direct(&self) -> Result<(Entity, WeaponSpec, WeaponTarget), Box<dyn std::error::Error>> {
        let shooter = self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter missing")?
            .clone();
        let target = self
            .world
            .entities
            .get(&self.target)
            .ok_or("target missing")?;
        let weapon = self
            .world
            .catalog
            .get(shooter.unit_type_id)?
            .weapon
            .clone()
            .ok_or("the scratch shooter has a weapon")?;
        let target_radius = self
            .world
            .catalog
            .get(target.unit_type_id)?
            .collision_radius;
        let shooter_radius = self
            .world
            .catalog
            .get(shooter.unit_type_id)?
            .collision_radius;
        let weapon_target = WeaponTarget {
            id: self.target,
            position: target.position,
            combined_radius: target_radius + shooter_radius,
        };
        Ok((shooter, weapon, weapon_target))
    }

    /// The target's remaining hit points, or zero once it is gone.
    fn target_hp(&self) -> SimScalar {
        self.world
            .entities
            .get(&self.target)
            .map_or(SimScalar::ZERO, |target| target.hp)
    }

    /// The shooter's weapon phase.
    fn phase(&self) -> Result<WeaponPhase, Box<dyn std::error::Error>> {
        Ok(self
            .world
            .entities
            .get(&self.shooter)
            .ok_or("shooter missing")?
            .weapon_phase
            .clone())
    }

    /// Steps one tick and reports whether a shot landed on the target in it.
    fn step(&mut self) -> bool {
        let before = self.target_hp();
        let tick = self.world.current_tick;
        self.world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        self.target_hp() < before
    }

    /// Steps `ticks` ticks and returns the ticks on which a shot landed.
    fn release_ticks(&mut self, ticks: u32) -> Vec<u32> {
        (0..ticks)
            .filter_map(|_| {
                let tick = self.world.current_tick;
                self.step().then_some(tick)
            })
            .collect()
    }
}

/// The gaps between consecutive releases.
fn gaps(releases: &[u32]) -> Vec<u32> {
    releases.windows(2).map(|pair| pair[1] - pair[0]).collect()
}

/// Asserts that the shooter releases at least four times and every gap is `period`.
fn assert_cadence(timings: Timings, stim: Option<f32>, period: u32) -> TestResult {
    let mut bench = Bench::new(timings, 10_000.0, stim)?;
    let releases = bench.release_ticks(120);
    assert!(releases.len() >= 4, "too few releases: {releases:?}");
    assert!(
        gaps(&releases).iter().all(|&gap| gap == period),
        "{timings:?} (stim {stim:?}) must release every {period} ticks, released on {releases:?}"
    );
    Ok(())
}

#[test]
fn a_backswing_longer_than_the_cooldown_does_not_delay_the_next_attack() -> TestResult {
    let timings = Timings {
        period: 10,
        damage_point: 1,
        backswing: 13,
    };
    assert_cadence(timings, None, 10)
}

#[test]
fn a_backswing_equal_to_the_cooldown_does_not_delay_the_next_attack() -> TestResult {
    let timings = Timings {
        period: 15,
        damage_point: 1,
        backswing: 14,
    };
    assert_cadence(timings, None, 15)
}

/// Battle 25b's mechanism (evidence `2026-11-03-battle-33-investigation.md` §5.7): an
/// unstimmed period of 15 hides a backswing of 13, a stimmed period of 10 does not.
#[test]
fn a_stimmed_weapon_fires_at_the_stimmed_period_whatever_its_backswing() -> TestResult {
    let timings = Timings {
        period: 15,
        damage_point: 1,
        backswing: 13,
    };
    assert_cadence(timings, None, 15)?;
    assert_cadence(timings, Some(1.5), 10)
}

/// Guard: a backswing shorter than the cooldown is untouched by the rule, tick by tick.
#[test]
fn a_short_backswing_is_unchanged() -> TestResult {
    let timings = Timings {
        period: 15,
        damage_point: 2,
        backswing: 4,
    };
    let mut bench = Bench::new(timings, 10_000.0, None)?;
    let target = bench.target;
    let windup = |remaining_ticks| WeaponPhase::Windup {
        target_id: target,
        remaining_ticks,
    };
    let backswing = |remaining_ticks| WeaponPhase::Backswing {
        target_id: target,
        remaining_ticks,
    };
    // Tick 0 started the swing: Windup 2. Then one full cycle and the start of the next.
    let mut expected = vec![(windup(1), false), (backswing(4), true)];
    expected.extend((1..4).rev().map(|remaining| (backswing(remaining), false)));
    expected.extend((0..9).map(|_| (WeaponPhase::Cooldown, false)));
    expected.extend([(windup(2), false), (windup(1), false), (backswing(4), true)]);
    assert_eq!(bench.phase()?, windup(2));
    for (tick, (phase, released)) in expected.into_iter().enumerate() {
        let landed = bench.step();
        assert_eq!(
            (bench.phase()?, landed),
            (phase, released),
            "tick {} of the sequence",
            tick + 1
        );
    }
    Ok(())
}

/// Guard of the lock: with nothing left to attack, the backswing runs to its end even though
/// the cooldown ran out long before.
#[test]
fn a_backswing_with_nothing_to_attack_runs_to_its_end() -> TestResult {
    let timings = Timings {
        period: 10,
        damage_point: 1,
        backswing: 13,
    };
    let mut bench = Bench::new(timings, 10.0, None)?;
    assert!(bench.step(), "the first shot lands on tick 1 and kills");
    assert!(
        !bench.world.entities.contains_key(&bench.target) || bench.target_hp() <= SimScalar::ZERO,
        "the one shot must kill the target"
    );
    for remaining in (1..=13).rev() {
        assert!(
            matches!(
                bench.phase()?,
                WeaponPhase::Backswing { remaining_ticks, .. } if remaining_ticks == remaining
            ),
            "expected Backswing {remaining}, got {:?}",
            bench.phase()?
        );
        bench.step();
    }
    assert_eq!(bench.phase()?, WeaponPhase::Ready);
    Ok(())
}

/// Asserts that loading the scratch catalog with `timings` is rejected naming `period_ticks`.
fn assert_period_rejected(timings: Timings) {
    let outcome = UnitCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        &catalog_ron(timings, 100.0, None),
    )
    .map(|_| ());
    assert!(
        matches!(
            &outcome,
            Err(SimCoreError::CatalogInvalidValue { definition, field, .. })
                if definition == SHOOTER && field == "weapon.period_ticks"
        ),
        "{timings:?} must be rejected naming weapon.period_ticks, got {outcome:?}"
    );
}

#[test]
fn a_period_not_longer_than_the_damage_point_is_rejected() -> TestResult {
    for (period, damage_point) in [(2, 2), (3, 5), (1, 1)] {
        assert_period_rejected(Timings {
            period,
            damage_point,
            backswing: 4,
        });
    }
    // The boundary: one tick of cooldown is a valid weapon.
    UnitCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        &catalog_ron(
            Timings {
                period: 3,
                damage_point: 2,
                backswing: 4,
            },
            100.0,
            None,
        ),
    )?;
    Ok(())
}

/// A damage point of 0 with a backswing shorter than the cooldown: the zealot's timings after
/// plan `2026-11-04-combat-sc2-alignment` WP4 (period 21, damage point 0, backswing 9).
const DAMAGE_POINT_ZERO: Timings = Timings {
    period: 21,
    damage_point: 0,
    backswing: 9,
};

/// Whether `phase` is a wind-up, which a damage point of 0 never shows (D-3).
const fn is_windup(phase: &WeaponPhase) -> bool {
    matches!(phase, WeaponPhase::Windup { .. })
}

#[test]
fn damage_point_zero_fires_every_period() -> TestResult {
    assert_cadence(DAMAGE_POINT_ZERO, None, 21)
}

#[test]
fn damage_point_zero_releases_on_the_tick_its_swing_starts() -> TestResult {
    // One call: the call that starts the swing releases it and charges the full cooldown.
    let bench = Bench::spawned(&catalog_ron(DAMAGE_POINT_ZERO, 10_000.0, None), 13)?;
    let (mut shooter, weapon, target) = bench.direct()?;
    assert!(
        step_entity_weapon(&mut shooter, &weapon, Some(target)),
        "the first call with the target in band and aligned must release, phase {:?}",
        shooter.weapon_phase
    );
    assert_eq!(
        shooter.weapon_phase,
        WeaponPhase::Backswing {
            target_id: bench.target,
            remaining_ticks: 9,
        }
    );
    assert_eq!(shooter.weapon_cooldown_remaining, 21);

    // In the world: the shot lands on the tick the attack order is given, and no tick ever
    // shows a wind-up.
    let mut bench = Bench::new(DAMAGE_POINT_ZERO, 10_000.0, None)?;
    assert_eq!(
        bench.target_hp(),
        SimScalar::from_i32(9_990),
        "the shot must land on tick 0"
    );
    for _ in 0..100 {
        bench.step();
        assert!(!is_windup(&bench.phase()?), "{:?}", bench.phase()?);
    }
    Ok(())
}

#[test]
fn damage_point_zero_after_turning_releases_on_the_tick_it_is_aligned() -> TestResult {
    // A turret at 10 degrees per tick with a 15 degree half arc, its target straight behind.
    let mount = "firing_arc_deg: 15.0, acquire_arc_deg: 15.0, has_turret: true, \
                 turret_turn_rate_deg_per_tick: 10.0";
    let catalog = catalog_ron_with_mount(DAMAGE_POINT_ZERO, mount, 10_000.0, None);
    let bench = Bench::spawned(&catalog, 7)?;
    let (mut shooter, weapon, target) = bench.direct()?;
    let mut first_aligned = None;
    let mut first_release = None;
    for call in 1..=40_u32 {
        let released = step_entity_weapon(&mut shooter, &weapon, Some(target));
        assert!(!is_windup(&shooter.weapon_phase), "call {call}");
        let aligned = is_within_firing_arc(
            shooter.turret_angle,
            shooter.position,
            target.position,
            weapon.firing_arc,
        );
        if aligned && first_aligned.is_none() {
            first_aligned = Some(call);
        }
        if released && first_release.is_none() {
            first_release = Some(call);
        }
    }
    assert!(
        first_aligned.is_some_and(|call| call > 1),
        "the turret must need several calls to align, aligned on {first_aligned:?}"
    );
    assert_eq!(
        first_release, first_aligned,
        "the swing must release on the call that aligns it"
    );
    Ok(())
}

#[test]
fn damage_point_zero_out_of_a_backswing_releases_at_once() -> TestResult {
    let timings = Timings {
        period: 21,
        damage_point: 0,
        backswing: 30,
    };
    let mut bench = Bench::new(timings, 10_000.0, None)?;
    let target = bench.target;
    let mut releases = Vec::new();
    for _ in 0..120 {
        let before = bench.phase()?;
        let tick = bench.world.current_tick;
        let landed = bench.step();
        let after = bench.phase()?;
        assert!(!is_windup(&after), "tick {tick}: {after:?}");
        if landed {
            assert!(
                matches!(before, WeaponPhase::Backswing { .. }),
                "tick {tick}: the release must come straight out of the backswing, was {before:?}"
            );
            assert_eq!(
                after,
                WeaponPhase::Backswing {
                    target_id: target,
                    remaining_ticks: 30,
                },
                "tick {tick}"
            );
            releases.push(tick);
        }
    }
    assert!(releases.len() >= 4, "too few releases: {releases:?}");
    assert!(
        gaps(&releases).iter().all(|&gap| gap == 21),
        "must release every 21 ticks, released on {releases:?}"
    );
    Ok(())
}

/// Guard: a damage point of 1 is untouched by D-3, tick by tick.
#[test]
fn damage_point_one_is_unchanged() -> TestResult {
    let timings = Timings {
        period: 21,
        damage_point: 1,
        backswing: 9,
    };
    let mut bench = Bench::new(timings, 10_000.0, None)?;
    let target = bench.target;
    let windup = WeaponPhase::Windup {
        target_id: target,
        remaining_ticks: 1,
    };
    let backswing = |remaining_ticks| WeaponPhase::Backswing {
        target_id: target,
        remaining_ticks,
    };
    // Tick 0 started the swing: Windup 1. Then one full cycle of 21 ticks and the next release.
    let mut expected = vec![(backswing(9), true)];
    expected.extend((1..9).rev().map(|remaining| (backswing(remaining), false)));
    expected.extend((0..11).map(|_| (WeaponPhase::Cooldown, false)));
    expected.extend([(windup.clone(), false), (backswing(9), true)]);
    assert_eq!(bench.phase()?, windup);
    for (tick, (phase, released)) in expected.into_iter().enumerate() {
        let landed = bench.step();
        assert_eq!(
            (bench.phase()?, landed),
            (phase, released),
            "tick {} of the sequence",
            tick + 1
        );
    }
    Ok(())
}

/// Guard: a wind-up with nothing left to wait for releases on its next step. The rule never
/// creates one after D-3; this pins the defensive path that would still release it.
#[test]
fn a_windup_with_no_ticks_left_releases_on_its_next_step() -> TestResult {
    let bench = Bench::spawned(&catalog_ron(DAMAGE_POINT_ZERO, 10_000.0, None), 13)?;
    let (mut shooter, weapon, target) = bench.direct()?;
    shooter.weapon_phase = WeaponPhase::Windup {
        target_id: bench.target,
        remaining_ticks: 0,
    };
    assert!(step_entity_weapon(&mut shooter, &weapon, Some(target)));
    assert_eq!(shooter.weapon_cooldown_remaining, 21);
    Ok(())
}

/// A turret at 11.25 degrees per tick (2 048 binary units, so turns are exact) with an engaged
/// half arc of 30 degrees and an acquisition
/// half arc of 0 (`Arc` 0, `ArcSlop` 30), so the two angles of B-29 are far apart.
const SPLIT_ARC_TURRET: &str = "firing_arc_deg: 30.0, acquire_arc_deg: 0.0, has_turret: true, \
     turret_turn_rate_deg_per_tick: 11.25";

/// The shooter's aim error against `target`, in binary units (65 536 per turn).
fn aim_error(shooter: &Entity, target: &WeaponTarget) -> u16 {
    let delta = target.position - shooter.position;
    shooter
        .turret_angle
        .diff(SimAngle::atan2(delta.y, delta.x))
        .unsigned_abs()
}

#[test]
fn b29_a_new_target_needs_the_acquisition_arc() -> TestResult {
    // The target straight behind: the turret turns 180 degrees, and the swing starts only when
    // it faces the target exactly (`Arc` ÷ 2 = 0), not once it is inside the engaged 30.
    let catalog = catalog_ron_with_mount(DAMAGE_POINT_ZERO, SPLIT_ARC_TURRET, 10_000.0, None);
    let bench = Bench::spawned(&catalog, 7)?;
    let (mut shooter, weapon, target) = bench.direct()?;
    let mut released_at = None;
    for call in 1..=30_u32 {
        let error = aim_error(&shooter, &target);
        let released = step_entity_weapon(&mut shooter, &weapon, Some(target));
        if released {
            released_at = Some(call);
            assert_eq!(aim_error(&shooter, &target), 0, "call {call}");
            break;
        }
        assert!(
            call == 1 || error > 0,
            "call {call}: an aimed weapon must have swung, error {error}"
        );
    }
    // 180 degrees at 11.25 per tick: aligned on the 16th call, not on the 14th (inside 30).
    assert_eq!(released_at, Some(16));
    Ok(())
}

#[test]
fn b29_an_engaged_weapon_fires_within_the_engaged_arc_without_turning() -> TestResult {
    let catalog = catalog_ron_with_mount(DAMAGE_POINT_ZERO, SPLIT_ARC_TURRET, 10_000.0, None);
    let bench = Bench::spawned(&catalog, 13)?;
    let (mut shooter, weapon, target) = bench.direct()?;
    assert!(step_entity_weapon(&mut shooter, &weapon, Some(target)));
    // Knock the turret 20 degrees off the target: inside the engaged arc.
    let offset = SimAngle::from_degrees(20);
    shooter.turret_angle += offset;
    let mut released = false;
    for _ in 0..21 {
        released = step_entity_weapon(&mut shooter, &weapon, Some(target));
    }
    assert!(
        released,
        "the engaged weapon must swing when its cooldown ends"
    );
    assert_eq!(shooter.turret_angle, offset, "and must not have turned");
    Ok(())
}

#[test]
fn b29_a_target_that_leaves_the_engaged_arc_is_acquired_again() -> TestResult {
    let catalog = catalog_ron_with_mount(DAMAGE_POINT_ZERO, SPLIT_ARC_TURRET, 10_000.0, None);
    let bench = Bench::spawned(&catalog, 13)?;
    let (mut shooter, weapon, target) = bench.direct()?;
    assert!(step_entity_weapon(&mut shooter, &weapon, Some(target)));
    // 45 degrees off: outside the engaged 30, so the weapon has lost the target and must face
    // it exactly again (4 calls at 11.25 degrees) before the next swing.
    shooter.turret_angle = SimAngle::from_degrees(45);
    let mut releases = Vec::new();
    for call in 1..=30_u32 {
        if step_entity_weapon(&mut shooter, &weapon, Some(target)) {
            releases.push((call, aim_error(&shooter, &target)));
        }
    }
    // The cooldown ends on call 21; the turret turns 33.75, 22.5, 11.25, 0: back inside the
    // engaged 30 on call 22, but the swing waits for the exact aim on call 24.
    assert_eq!(releases.first(), Some(&(24, 0)), "{releases:?}");
    Ok(())
}
