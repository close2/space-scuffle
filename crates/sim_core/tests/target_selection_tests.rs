//! Behavioural tests for automatic target selection, criterion by criterion.
//!
//! # Responsibility
//! Pins the ranking of plan `2026-11-05-sc2-deviation-fixes` WP4, which follows `StarCraft` II's
//! automatic targeting (Liquipedia "Automatic Targeting"; `CUnit` `AttackTargetPriority`,
//! `AlwaysThreatens`, `DefaultAcquireLevel`, `WeaponArray`; `CWeapon` `AcquirePrioritization`):
//! 1. threat: the candidate has a weapon that can hit the chooser, or is flagged always a threat;
//!    a unit with the defensive acquire level threatens only while it attack-moves;
//! 2. attack target priority, higher first;
//! 3. the chooser's primary weapon;
//! 4. only when the chooser has no target it keeps: its tie-break (closest, least turning, or
//!    closest to the previous target).
//!
//! # How it fits
//! The tests drive the public world API (`SimWorld::step`) on a flat map and read the order
//! acquisition gives the chooser, as a battle sees it.
//!
//! It also pins who may fire outside an attack order (plan `2026-11-23-turret-wind-up` D-2 to
//! D-4, P-1 to P-3): a weapon works against `current_target` only under an attack order or on
//! hold, unless its data clears `StarCraft` II's `OnlyFireWhileInAttackOrder`
//! (`fires_outside_attack_order`); such a weapon fires while its unit's order walks it only when
//! it is `Moving`, and acquisition tracks its target from where the unit is, never changing the
//! unit's order. The mount plays no part.
//!
//! # Invariants
//! Every archetype is a scratch archetype written in this file, never a shipped identity (C5),
//! so a rebalance of the shipped catalog cannot break these tests. Positions are world units.

use sim_core::catalog::UnitCatalog;
use sim_core::SimWorld;
use sim_core::WeaponPhase;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, PlayerCommand, PlayerId, ResourceNodeId, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A 6-range, 1-damage weapon that hits every attribute, in the catalog's RON form.
const RIFLE: &str = r#"Some((
            name: "Scratch Rifle",
            visual_kind: Hitscan,
            target_flags: (bits: 255),
            min_range: 0.0,
            max_range: 6.0,
            firing_arc_deg: 180.0,
            acquire_arc_deg: 180.0,
            has_turret: false,
            turret_turn_rate_deg_per_tick: 0.0,
            turret_idle: Hold,
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
        ))"#;

/// One scratch archetype.
struct Scratch<'a> {
    /// Slug, unique in the scratch catalog.
    slug: &'a str,
    /// Mover type in RON (`Ground` or `Air`).
    mover: &'a str,
    /// `attack_target_priority`.
    priority: u8,
    /// `primary_weapon` in RON (`Either`, `Ground` or `Air`).
    primary: &'a str,
    /// `acquire_tie_break` in RON (`Closest`, `LeastTurning` or `ClosestToPrevious`).
    tie_break: &'a str,
    /// Extra authored targeting fields, for example `always_a_threat: true,`.
    extra: &'a str,
    /// The `ground_weapon` slot in RON.
    ground: &'a str,
    /// The `air_weapon` slot in RON.
    air: &'a str,
    /// `scan_range` in world units, in RON.
    scan: &'a str,
    /// `collision_radius` (and `separation_radius`) in world units, in RON.
    radius: &'a str,
    /// `max_speed` in world units per tick, in RON.
    speed: &'a str,
}

impl Scratch<'_> {
    /// An unarmed ground archetype of priority 20.
    const fn unarmed(slug: &str) -> Scratch<'_> {
        Scratch {
            slug,
            mover: "Ground",
            priority: 20,
            primary: "Either",
            tie_break: "Closest",
            extra: "",
            ground: "None",
            air: "None",
            scan: "12.0",
            radius: "0.5",
            speed: "0.1",
        }
    }

    /// A ground archetype of priority 20 armed with [`RIFLE`] against ground targets.
    const fn gunner(slug: &str) -> Scratch<'_> {
        Scratch {
            ground: RIFLE,
            ..Scratch::unarmed(slug)
        }
    }

    /// The archetype in the catalog's RON form, with unit type `id`.
    fn ron(&self, id: u16) -> String {
        format!(
            r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of target_selection_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: {mover},
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
            max_speed: {speed},
            acceleration: 0.1,
            deceleration: 0.1,
            turn_rate_deg_per_tick: 180.0,
            collision_radius: {radius},
            separation_radius: {radius},
            push_priority: 20,
            sight_radius: 12.0,
            scan_range: {scan},
            attack_target_priority: {priority},
            primary_weapon: {primary},
            acquire_tie_break: {tie_break},
            {extra}
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 1,
            supply_provided: 0,
            build_time_ticks: 100,
            ground_weapon: {ground},
            air_weapon: {air},
        )"#,
            slug = self.slug,
            mover = self.mover,
            priority = self.priority,
            primary = self.primary,
            tie_break = self.tie_break,
            extra = self.extra,
            ground = self.ground,
            air = self.air,
            scan = self.scan,
            radius = self.radius,
            speed = self.speed,
        )
    }
}

/// A world on a flat 48 × 48 map whose catalog holds exactly `archetypes`.
fn world_with(archetypes: &[Scratch<'_>]) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 7);
    let units: Vec<String> = archetypes
        .iter()
        .zip(1_u16..)
        .map(|(archetype, id)| archetype.ron(id))
        .collect();
    world.catalog = UnitCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        &format!("(units: [{}])", units.join(", ")),
    )?;
    Ok(world)
}

/// Spawns the archetype `slug` for `owner` at (`x`, `y`).
fn spawn(
    world: &mut SimWorld,
    slug: &str,
    owner: u8,
    x: i32,
    y: i32,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let unit_type = world.catalog.id_by_slug(slug).ok_or(slug.to_owned())?;
    let at = SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    Ok(world.spawn_unit(unit_type, PlayerId(owner), at)?)
}

/// Steps `world` once at `tick`, issuing `order` to `unit` first when given.
fn step(world: &mut SimWorld, tick: u32, order: Option<(EntityId, UnitOrder)>) {
    let commands = order.map_or_default(|(unit, order)| {
        vec![PlayerCommand {
            player_id: world.entities.get(&unit).map_or(PlayerId(0), |e| e.owner),
            entities: vec![unit],
            order,
            queue: false,
        }]
    });
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
}

/// The enemy `unit`'s automatic attack order targets, if its front order is one.
fn acquired(world: &SimWorld, unit: EntityId) -> Option<EntityId> {
    match world.entities.get(&unit)?.order_queue.front()? {
        UnitOrder::AttackTarget {
            target,
            origin: AttackOrigin::Acquired,
        } => Some(*target),
        _ => None,
    }
}

/// The chooser's slug in every test.
const CHOOSER: &str = "chooser";

/// B-7: an armed enemy whose weapon cannot hit the chooser is no threat, so a farther enemy
/// that can hit it is chosen over it.
#[test]
fn b7_an_enemy_whose_weapon_cannot_hit_me_is_no_threat() -> TestResult {
    let mut world = world_with(&[
        Scratch::gunner(CHOOSER),
        Scratch {
            ground: "None",
            air: RIFLE,
            ..Scratch::unarmed("air_gunner")
        },
        Scratch::gunner("ground_gunner"),
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    spawn(&mut world, "air_gunner", 1, 14, 16)?;
    let threat = spawn(&mut world, "ground_gunner", 1, 10, 22)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(threat));
    Ok(())
}

/// B-7: an unarmed enemy flagged always a threat ranks as a threat, ahead of a closer unarmed
/// enemy of the same priority.
#[test]
fn b7_an_enemy_flagged_always_a_threat_is_a_threat_unarmed() -> TestResult {
    let mut world = world_with(&[
        Scratch::gunner(CHOOSER),
        Scratch::unarmed("plain"),
        Scratch {
            extra: "always_a_threat: true,",
            ..Scratch::unarmed("flagged")
        },
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    spawn(&mut world, "plain", 1, 14, 16)?;
    let flagged = spawn(&mut world, "flagged", 1, 10, 22)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(flagged));
    Ok(())
}

/// B-7: an enemy's current order does not make it more of a threat. Of two armed enemies that
/// can hit the chooser, the closer is chosen although only the farther is attack-moving.
#[test]
fn b7_an_attacking_enemy_is_no_greater_threat_than_an_idle_one() -> TestResult {
    let mut world = world_with(&[
        Scratch::gunner(CHOOSER),
        Scratch::gunner("idle_gunner"),
        Scratch::gunner("moving_gunner"),
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let idle = spawn(&mut world, "idle_gunner", 1, 14, 16)?;
    let moving = spawn(&mut world, "moving_gunner", 1, 10, 22)?;
    let away = SimVec2::new(SimScalar::from_i32(10), SimScalar::from_i32(40));
    step(
        &mut world,
        0,
        Some((moving, UnitOrder::AttackMove { target: away })),
    );
    assert_eq!(acquired(&world, chooser), Some(idle));
    Ok(())
}

/// B-7: an enemy with the defensive acquire level (the workers') is a threat only while it
/// attack-moves.
#[test]
fn b7_a_defensive_enemy_threatens_only_while_attack_moving() -> TestResult {
    let archetypes = [
        Scratch::gunner(CHOOSER),
        Scratch {
            extra: "acquire_level: Defensive,",
            ..Scratch::gunner("worker")
        },
        Scratch::gunner("soldier"),
    ];
    let mut world = world_with(&archetypes)?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    spawn(&mut world, "worker", 1, 14, 16)?;
    let soldier = spawn(&mut world, "soldier", 1, 10, 22)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(soldier), "an idle worker");

    let mut world = world_with(&archetypes)?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let worker = spawn(&mut world, "worker", 1, 14, 16)?;
    spawn(&mut world, "soldier", 1, 10, 22)?;
    let away = SimVec2::new(SimScalar::from_i32(14), SimScalar::from_i32(40));
    step(
        &mut world,
        0,
        Some((worker, UnitOrder::AttackMove { target: away })),
    );
    assert_eq!(
        acquired(&world, chooser),
        Some(worker),
        "an attack-moving worker"
    );
    Ok(())
}

/// B-10: of two threats of equal priority, a unit whose primary weapon is its air weapon takes
/// the air one, although the ground one is closer.
#[test]
fn b10_a_unit_prefers_a_target_for_its_primary_weapon() -> TestResult {
    let mut world = world_with(&[
        Scratch {
            air: RIFLE,
            primary: "Air",
            ..Scratch::gunner(CHOOSER)
        },
        Scratch::gunner("ground_gunner"),
        Scratch {
            mover: "Air",
            ..Scratch::gunner("air_gunner")
        },
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    spawn(&mut world, "ground_gunner", 1, 14, 16)?;
    let flyer = spawn(&mut world, "air_gunner", 1, 10, 22)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(flyer));
    Ok(())
}

/// B-10: threat outranks the primary weapon: an unarmed air enemy gives way to a ground threat.
#[test]
fn b10_threat_outranks_the_primary_weapon() -> TestResult {
    let mut world = world_with(&[
        Scratch {
            air: RIFLE,
            primary: "Air",
            ..Scratch::gunner(CHOOSER)
        },
        Scratch::gunner("ground_gunner"),
        Scratch {
            mover: "Air",
            ..Scratch::unarmed("air_drone")
        },
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let gunner = spawn(&mut world, "ground_gunner", 1, 10, 22)?;
    spawn(&mut world, "air_drone", 1, 14, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(gunner));
    Ok(())
}

/// Moves `unit` to (`x`, `y`) between ticks.
fn teleport(world: &mut SimWorld, unit: EntityId, x: i32, y: i32) -> TestResult {
    let entity = world.entities.get_mut(&unit).ok_or("teleported unit")?;
    entity.position = SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    entity.pos_at_tick_start = entity.position;
    Ok(())
}

/// B-9: a target in weapon range is kept when a closer candidate of the same rank appears.
#[test]
fn b9_a_target_in_range_is_kept_against_a_closer_equal_candidate() -> TestResult {
    let mut world = world_with(&[Scratch::gunner(CHOOSER), Scratch::unarmed("target")])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let first = spawn(&mut world, "target", 1, 15, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(first));
    spawn(&mut world, "target", 1, 12, 16)?;
    step(&mut world, 1, None);
    assert_eq!(acquired(&world, chooser), Some(first));
    Ok(())
}

/// B-9: a target in weapon range gives way to a candidate of higher rank (here a threat).
#[test]
fn b9_a_target_in_range_gives_way_to_a_higher_ranked_candidate() -> TestResult {
    let mut world = world_with(&[
        Scratch::gunner(CHOOSER),
        Scratch::unarmed("target"),
        Scratch::gunner("threat"),
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let first = spawn(&mut world, "target", 1, 15, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(first));
    let threat = spawn(&mut world, "threat", 1, 10, 26)?;
    step(&mut world, 1, None);
    assert_eq!(acquired(&world, chooser), Some(threat));
    Ok(())
}

/// B-9: once its target is out of weapon range, the unit takes the closest candidate of the
/// best rank, with no margin in favour of the old target. The old target, 9 away, gives way to
/// one 8 away; both are beyond reach (the weapon's 6 plus both radii, 7).
#[test]
fn b9_a_target_out_of_range_gives_way_to_the_closest_candidate() -> TestResult {
    let mut world = world_with(&[Scratch::gunner(CHOOSER), Scratch::unarmed("target")])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let first = spawn(&mut world, "target", 1, 15, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(first));
    teleport(&mut world, chooser, 10, 16)?;
    teleport(&mut world, first, 19, 16)?;
    let closer = spawn(&mut world, "target", 1, 10, 24)?;
    step(&mut world, 1, None);
    assert_eq!(acquired(&world, chooser), Some(closer));
    Ok(())
}

/// B-11: a unit whose tie-break is the least turning takes, of two equal candidates, the one
/// straight ahead of its facing (+x at spawn), although the other is closer.
#[test]
fn b11_least_turning_takes_the_candidate_ahead() -> TestResult {
    let mut world = world_with(&[
        Scratch {
            tie_break: "LeastTurning",
            ..Scratch::gunner(CHOOSER)
        },
        Scratch::unarmed("target"),
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    spawn(&mut world, "target", 1, 10, 20)?;
    let ahead = spawn(&mut world, "target", 1, 16, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(ahead));
    Ok(())
}

/// B-11: once its target is lost, a unit whose tie-break is the previous target takes, of two
/// equal candidates, the one closest to where that target stood, although the other is closer
/// to the unit.
#[test]
fn b11_closest_to_previous_takes_the_candidate_nearest_the_lost_target() -> TestResult {
    let mut world = world_with(&[
        Scratch {
            tie_break: "ClosestToPrevious",
            ..Scratch::gunner(CHOOSER)
        },
        Scratch::unarmed("target"),
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let first = spawn(&mut world, "target", 1, 15, 16)?;
    step(&mut world, 0, None);
    step(&mut world, 1, None);
    assert_eq!(acquired(&world, chooser), Some(first));
    // The target leaves scan range at once: lost, as if it had died.
    teleport(&mut world, first, 40, 40)?;
    spawn(&mut world, "target", 1, 10, 20)?;
    let near_first = spawn(&mut world, "target", 1, 17, 19)?;
    step(&mut world, 2, None);
    assert_eq!(acquired(&world, chooser), Some(near_first));
    Ok(())
}

/// B-80: a unit with the defensive acquire level (a worker) leaves an enemy in its reach alone
/// while idle, and attacks it while attack-moving.
#[test]
fn b80_a_defensive_unit_acquires_only_while_attack_moving() -> TestResult {
    let archetypes = [
        Scratch {
            extra: "acquire_level: Defensive,",
            ..Scratch::gunner(CHOOSER)
        },
        Scratch::unarmed("target"),
    ];
    let mut world = world_with(&archetypes)?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    spawn(&mut world, "target", 1, 14, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), None, "idle");

    let mut world = world_with(&archetypes)?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let target = spawn(&mut world, "target", 1, 14, 16)?;
    let beyond = SimVec2::new(SimScalar::from_i32(30), SimScalar::from_i32(16));
    step(
        &mut world,
        0,
        Some((chooser, UnitOrder::AttackMove { target: beyond })),
    );
    assert_eq!(acquired(&world, chooser), Some(target), "attack-moving");
    Ok(())
}

/// A gunner ([`RIFLE`], reach 6 beyond both edges) scanning 7 beyond both edges, as `StarCraft`
/// II measures `MinScanRange` (Q0057).
fn scanner() -> Scratch<'static> {
    Scratch {
        scan: "7.0",
        ..Scratch::gunner(CHOOSER)
    }
}

/// B-12 (Q0057): scan range is measured edge to edge. An enemy 8 between centres is 7 beyond
/// both radii (0.5 each), exactly the scan range, so it is acquired; between centres it lies
/// beyond the scan range.
#[test]
fn b12_scan_range_is_measured_from_the_edges() -> TestResult {
    let mut world = world_with(&[scanner(), Scratch::unarmed("target")])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let target = spawn(&mut world, "target", 1, 18, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(target));
    Ok(())
}

/// B-12 (Q0057): an enemy 9 between centres, 8 beyond both edges, is outside the scan range of
/// 7 and is left alone.
#[test]
fn b12_an_enemy_beyond_the_scan_range_from_the_edges_is_left_alone() -> TestResult {
    let mut world = world_with(&[scanner(), Scratch::unarmed("target")])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    spawn(&mut world, "target", 1, 19, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), None);
    Ok(())
}

/// B-12 (Q0057): a large enemy is scanned from its edge. One of radius 2.5, 10 between centres,
/// is 7 beyond both edges and is acquired, although its centre lies 2.5 beyond the chooser's
/// scan range plus its own radius.
#[test]
fn b12_a_large_enemy_is_scanned_from_its_edge() -> TestResult {
    let mut world = world_with(&[
        scanner(),
        Scratch {
            radius: "2.5",
            ..Scratch::unarmed("hulk")
        },
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let hulk = spawn(&mut world, "hulk", 1, 20, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(hulk));
    Ok(())
}

/// B-12 (audit §11.1): a weapon scans at least to its own range. A gunner scanning 3 acquires
/// an enemy 6 beyond both edges, inside its weapon's range of 6, as `StarCraft` II's
/// `MinScanRange` "implicitly becomes equivalent to the current range" when the range is higher.
#[test]
fn b12_a_weapon_scans_at_least_to_its_range() -> TestResult {
    let mut world = world_with(&[
        Scratch {
            scan: "3.0",
            ..Scratch::gunner(CHOOSER)
        },
        Scratch::unarmed("target"),
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let target = spawn(&mut world, "target", 1, 17, 16)?;
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(target));
    Ok(())
}

/// B-12 (audit §11.3): a sieged unit scans to its siege weapon's range. The tank-mode scan of 7
/// is the unit's value; sieged, with a weapon of range 13, it acquires an enemy 13 beyond both
/// edges (`StarCraft` II's Siege Mode `CrucioShockCannon`, range 13, no `MinScanRange`).
#[test]
fn b12_a_sieged_unit_scans_to_its_siege_weapon_range() -> TestResult {
    let mut world = world_with(&[scanner(), Scratch::unarmed("target")])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let target = spawn(&mut world, "target", 1, 24, 16)?;
    // An unarmed ally beside the target reveals it: it lies beyond the tank's sight of 12.
    spawn(&mut world, "target", 0, 24, 19)?;
    let unit_type = world.catalog.id_by_slug(CHOOSER).ok_or(CHOOSER)?;
    let mut cannon = world
        .catalog
        .get(unit_type)?
        .ground_weapon
        .clone()
        .ok_or("the scanner has a ground weapon")?;
    cannon.max_range = SimScalar::from_i32(13);
    let tank = world.entities.get_mut(&chooser).ok_or("tank")?;
    tank.is_sieged = true;
    tank.is_immobile = true;
    tank.siege_weapon = Some(cannon);
    step(&mut world, 0, None);
    assert_eq!(acquired(&world, chooser), Some(target));
    Ok(())
}

/// [`RIFLE`] on a turret (`turret` true, 180° a tick) or the hull, with `rules`: authored
/// `allowed_movement` and `fires_outside_attack_order` fields, or nothing for the defaults.
fn rifle(turret: bool, rules: &str) -> String {
    let mount = if turret {
        "has_turret: true,\n            turret_turn_rate_deg_per_tick: 180.0,"
    } else {
        "has_turret: false,\n            turret_turn_rate_deg_per_tick: 0.0,"
    };
    RIFLE
        .replace(
            "has_turret: false,\n            turret_turn_rate_deg_per_tick: 0.0,",
            mount,
        )
        .replace("turret_idle: Hold,", &format!("turret_idle: Hold, {rules}"))
}

/// The order classes of plan `2026-11-23-turret-wind-up` P-1, each a front order queue and
/// whether the unit's task walks to a goal.
fn order_cases(enemy: EntityId) -> [(&'static str, Vec<UnitOrder>, bool); 6] {
    let away = SimVec2::new(SimScalar::from_i32(10), SimScalar::from_i32(40));
    [
        ("Move", vec![UnitOrder::Move { target: away }], false),
        (
            "AttackMove with no attack in front",
            vec![UnitOrder::AttackMove { target: away }],
            false,
        ),
        (
            "Stop, then a walking order",
            vec![UnitOrder::Stop, UnitOrder::Move { target: away }],
            false,
        ),
        (
            "a task order walking to its goal",
            vec![UnitOrder::Harvest {
                target: ResourceNodeId(0),
            }],
            true,
        ),
        (
            "AttackTarget",
            vec![UnitOrder::AttackTarget {
                target: enemy,
                origin: AttackOrigin::Explicit,
            }],
            false,
        ),
        ("HoldPosition", vec![UnitOrder::HoldPosition], false),
    ]
}

/// Whether a chooser armed with `ground`, whose `current_target` is an unarmed enemy 4 east (in
/// reach), starts a swing in 10 runs of the combat phase alone under `orders` (its task walking
/// to a goal when `task_goal`). Acquisition and movement do not run, so the order and the target
/// stay as set: this reads the combat phase's own choice (D-2).
fn swings_under(
    ground: &str,
    orders: impl FnOnce(EntityId) -> (Vec<UnitOrder>, bool),
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut world = world_with(&[
        Scratch {
            ground,
            ..Scratch::unarmed(CHOOSER)
        },
        Scratch::unarmed("enemy"),
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 10, 16)?;
    let enemy = spawn(&mut world, "enemy", 1, 14, 16)?;
    let (orders, task_goal) = orders(enemy);
    let unit = world.entities.get_mut(&chooser).ok_or("chooser")?;
    unit.order_queue = orders.into_iter().collect();
    unit.current_target = Some(enemy);
    if task_goal {
        unit.task.goal = Some(SimVec2::new(
            SimScalar::from_i32(30),
            SimScalar::from_i32(16),
        ));
    }
    let mut swung = false;
    // Vision before combat, as `step` orders the phases: a weapon strikes only what its player
    // sees (plan `2026-11-24-weapon-validation` D-3).
    world.update_vision();
    for _ in 0..10 {
        world.step_weapons_and_projectiles();
        let unit = world.entities.get(&chooser).ok_or("chooser")?;
        swung |= !matches!(
            unit.weapon_phase,
            WeaponPhase::Ready | WeaponPhase::RotatingToTarget { .. }
        );
    }
    Ok(swung)
}

/// P-1: a weapon that keeps `OnlyFireWhileInAttackOrder` (the default) works against the unit's
/// `current_target` only under an attack order or on hold, turreted or not: a move, an
/// attack-move with no attack in front, a stop before a walking order and a task order start no
/// swing (B-90).
#[test]
fn b90_a_weapon_fires_at_its_current_target_only_in_an_attack_order() -> TestResult {
    for turret in [false, true] {
        let weapon = rifle(turret, "");
        for index in 0..6 {
            let mut name = "";
            let swung = swings_under(&weapon, |enemy| {
                let (case, orders, goal) = order_cases(enemy)[index].clone();
                name = case;
                (orders, goal)
            })?;
            let in_attack_order = matches!(name, "AttackTarget" | "HoldPosition");
            assert_eq!(
                swung, in_attack_order,
                "turret {turret}, {name}: a swing only in an attack order"
            );
        }
    }
    Ok(())
}

/// P-2: a weapon with `fires_outside_attack_order` fires at its `current_target` under an order
/// that does not walk its unit; under one that walks it, only when it is `Moving` (D-2, D-3).
#[test]
fn b90_a_weapon_outside_an_attack_order_fires_while_walking_only_when_moving() -> TestResult {
    let walking = |_| {
        let away = SimVec2::new(SimScalar::from_i32(10), SimScalar::from_i32(40));
        (vec![UnitOrder::Move { target: away }], false)
    };
    // A harvester at its node: a task order with no goal to walk to.
    let standing = |_| {
        (
            vec![UnitOrder::Harvest {
                target: ResourceNodeId(0),
            }],
            false,
        )
    };
    for (movement, fires_while_walking) in
        [("Stopped", false), ("Slowing", false), ("Moving", true)]
    {
        let weapon = rifle(
            false,
            &format!("allowed_movement: {movement}, fires_outside_attack_order: true,"),
        );
        assert_eq!(
            swings_under(&weapon, walking)?,
            fires_while_walking,
            "{movement}: a swing under a walking order only when Moving"
        );
        assert!(
            swings_under(&weapon, standing)?,
            "{movement}: a swing under an order that does not walk"
        );
    }
    let moving_only = rifle(false, "allowed_movement: Moving,");
    assert!(
        !swings_under(&moving_only, walking)?,
        "Moving without the option still needs an attack order"
    );
    Ok(())
}

/// The front order and current target of `unit`.
fn orders_of(
    world: &SimWorld,
    unit: EntityId,
) -> Result<(Option<UnitOrder>, Option<EntityId>), Box<dyn std::error::Error>> {
    let entity = world.entities.get(&unit).ok_or("unit")?;
    Ok((entity.order_queue.front().cloned(), entity.current_target))
}

/// P-3: under a move, a unit whose weapon is `Moving` and fires outside an attack order tracks the
/// best enemy it reaches from where it is (by the acquisition ranking), never one beyond reach,
/// never changes its order, and releases the target once it has left reach. A unit with a
/// default weapon keeps its stale `current_target` untouched under the same order (D-4).
#[test]
fn b45_a_unit_that_fires_while_moving_tracks_the_best_enemy_in_reach() -> TestResult {
    let free = rifle(
        false,
        "allowed_movement: Moving, fires_outside_attack_order: true,",
    );
    let mut world = world_with(&[
        Scratch {
            ground: &free,
            speed: "0.5",
            ..Scratch::unarmed(CHOOSER)
        },
        Scratch {
            speed: "0.5",
            ..Scratch::gunner("plain")
        },
        Scratch::unarmed("low"),
        Scratch {
            priority: 30,
            ..Scratch::unarmed("high")
        },
        Scratch {
            priority: 40,
            ..Scratch::unarmed("far")
        },
    ])?;
    let chooser = spawn(&mut world, CHOOSER, 0, 20, 24)?;
    let plain = spawn(&mut world, "plain", 0, 20, 20)?;
    let low = spawn(&mut world, "low", 1, 24, 24)?;
    let high = spawn(&mut world, "high", 1, 24, 26)?;
    // Beyond reach (9 from the chooser's centre, reach 6 + 1) but within its scan range.
    let far = spawn(&mut world, "far", 1, 20, 33)?;
    let goal = |y| UnitOrder::Move {
        target: SimVec2::new(SimScalar::from_i32(2), SimScalar::from_i32(y)),
    };
    let command = |unit, order| PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![unit],
        order,
        queue: false,
    };
    world.step(&TickBundle {
        tick: Tick(0),
        commands: vec![command(chooser, goal(24)), command(plain, goal(20))],
        state_checksum: None,
    });
    // A target left over from before the move, as a unit that fought and was moved away has.
    world
        .entities
        .get_mut(&plain)
        .ok_or("plain")?
        .current_target = Some(low);
    assert_eq!(
        orders_of(&world, chooser)?,
        (Some(goal(24)), Some(high)),
        "the best enemy in reach, the order kept"
    );
    let full_hp = SimScalar::from_i32(10_000);
    let mut released_at = None;
    for tick in 1..80 {
        step(&mut world, tick, None);
        let (front, target) = orders_of(&world, chooser)?;
        assert!(
            front.is_none() || front == Some(goal(24)),
            "tick {tick}: the move stays the unit's order"
        );
        assert_ne!(
            target,
            Some(far),
            "tick {tick}: never an enemy beyond reach"
        );
        if target.is_none() && released_at.is_none() {
            released_at = Some(tick);
        }
        if released_at.is_some() {
            assert_eq!(
                target, None,
                "tick {tick}: released for good once out of reach"
            );
        }
        assert_eq!(
            orders_of(&world, plain)?.1,
            Some(low),
            "tick {tick}: a default weapon's stale target is left as it was"
        );
        assert!(
            matches!(
                world.entities.get(&plain).ok_or("plain")?.weapon_phase,
                WeaponPhase::Ready | WeaponPhase::RotatingToTarget { .. }
            ),
            "tick {tick}: a default weapon does not fire at its stale target"
        );
    }
    assert!(released_at.is_some(), "the target is released out of reach");
    let hp = |id| world.entities.get(&id).map(|e| e.hp);
    assert!(
        hp(high).is_some_and(|hp| hp < full_hp),
        "the tracked enemy was fired at"
    );
    assert_eq!(hp(far), Some(full_hp));
    Ok(())
}
