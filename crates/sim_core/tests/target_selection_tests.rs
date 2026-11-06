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
//! # Invariants
//! Every archetype is a scratch archetype written in this file, never a shipped identity (C5),
//! so a rebalance of the shipped catalog cannot break these tests. Positions are world units.

use sim_core::catalog::UnitCatalog;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder,
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
            has_turret: false,
            turret_turn_rate_deg_per_tick: 0.0,
            base_damage: 1.0,
            bonus_armored: 0.0,
            bonus_light: 0.0,
            attacks_count: 1,
            period_ticks: 20,
            damage_point_ticks: 2,
            backswing_ticks: 0,
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
            max_speed: 0.1,
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
            is_detector: false,
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
    let commands = order
        .map(|(unit, order)| {
            vec![PlayerCommand {
                player_id: world.entities.get(&unit).map_or(PlayerId(0), |e| e.owner),
                entities: vec![unit],
                order,
                queue: false,
            }]
        })
        .unwrap_or_default();
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
