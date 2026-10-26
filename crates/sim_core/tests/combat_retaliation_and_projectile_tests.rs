//! # Integration Tests: Projectile Target Loss Behaviors, Seeking Retargeting & Missile Timeout
//!
//! Verifies:
//! 1. `ProjectileTargetLossBehavior::Fizzle` nullifies projectile damage when the target becomes unavailable
//!    mid-flight across multiple causes (cargo pickup, cloaking without detector vision, invulnerability, or death).
//! 2. `ProjectileTargetLossBehavior::RetargetNearest` dynamically searches the spatial neighborhood and locks
//!    onto the nearest available alternate enemy when the primary target becomes unavailable mid-flight.
//! 3. Mandatory missile lifetime timeout (`remaining_lifetime_ticks`): projectiles that fail to reach their target
//!    before fuel/timer expiration cleanly despawn with 0 damage.
//! 4. `ProjectileTargetLossBehavior::ImpactLastKnownPosition` continues to ground coordinates and detonates splash
//!    damage even when the original target entity has vanished.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::{SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttributeFlags, EntityId, PlayerId, ProjectileTargetLossBehavior, SimAngle, SimScalar, SimVec2,
    Tick, TickBundle, UnitOrder, UnitTypeId, WeaponVisualKind, DEFAULT_PROJECTILE_LIFETIME_TICKS,
};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

fn create_test_world(seed: u64) -> SimWorld {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    SimWorld::new(map, seed)
}

/// Helper that creates a custom weapon specification with configurable target loss behavior and lifetime.
fn custom_projectile_weapon(
    speed: SimScalar,
    target_loss_behavior: ProjectileTargetLossBehavior,
    max_lifetime: u16,
    splash: Option<SplashSpec>,
) -> WeaponSpec {
    WeaponSpec {
        name: "Test Missile Battery".to_string(),
        visual_kind: WeaponVisualKind::Projectile,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(20),
        firing_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        base_damage: SimScalar::from_num(30),
        bonus_armored: SimScalar::ZERO,
        bonus_light: SimScalar::ZERO,
        attacks_count: 1,
        period_ticks: 30,
        damage_point_ticks: 1,
        backswing_ticks: 2,
        splash,
        projectile_speed: Some(speed),
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior,
        max_projectile_lifetime_ticks: max_lifetime,
    }
}

/// Helper to register an unarmed target dummy archetype.
///
/// Spawning targets with this dummy prevents them from counter-attacking or generating
/// extraneous projectiles, ensuring strict test isolation.
fn register_dummy_target(world: &mut SimWorld) -> UnitTypeId {
    let dummy_id = UnitTypeId(88);
    let pulse_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let mut dummy_arch = world.catalog.get(pulse_id).unwrap().clone();
    dummy_arch.unit_type_id = dummy_id;
    dummy_arch.slug = "target_dummy".to_string();
    dummy_arch.name = "Target Dummy".to_string();
    dummy_arch.display_name = "Target Dummy".to_string();
    dummy_arch.weapon = None;
    dummy_arch.ground_weapon = None;
    dummy_arch.air_weapon = None;
    world.catalog.register(dummy_arch);
    dummy_id
}

/// Test 1: Projectiles with `Fizzle` behavior disjoint and deal 0 damage when the target becomes
/// unavailable mid-flight due to:
/// (a) Cargo pickup (`is_loaded_in_transport = true`),
/// (b) Cloaking without active detector vision (`is_cloaked = true`),
/// (c) Invulnerability (`is_invulnerable = true`).
#[test]
fn test_projectile_target_loss_fizzle_on_cargo_cloak_and_invulnerability() {
    let pulse_slug = "pulse_bot";

    // Subcase (a): Cargo pickup
    {
        let mut world = create_test_world(0x10A);
        let pulse_id = world.catalog.id_by_slug(pulse_slug).expect(pulse_slug);

        let shooter = world
            .spawn_unit(
                pulse_id,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
            )
            .expect("spawn shooter");
        let target = world
            .spawn_unit(
                pulse_id,
                PlayerId(1),
                SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(16)),
            )
            .expect("spawn target");

        // Equip shooter with finite-speed homing projectile weapon
        let weapon = custom_projectile_weapon(
            SimScalar::from_f32_lossy(0.60),
            ProjectileTargetLossBehavior::Fizzle,
            DEFAULT_PROJECTILE_LIFETIME_TICKS,
            None,
        );
        let mut arch: UnitArchetype = world.catalog.get(pulse_id).unwrap().clone();
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
        world.catalog.register(arch);

        let initial_hp = world.entities.get(&target).unwrap().hp;
        world
            .entities
            .get_mut(&shooter)
            .unwrap()
            .issue_order(UnitOrder::AttackTarget { target }, false);

        // Step until projectile launches
        let mut launched = false;
        let mut launch_tick = 1;
        for t in 1..=10 {
            world.step(&empty_bundle(t));
            if !world.projectiles.is_empty() {
                launched = true;
                launch_tick = t;
                break;
            }
        }
        assert!(launched, "Subcase (a): Projectile must be in flight");

        // Target loaded into transport mid-flight
        world.load_unit_into_transport(target, EntityId(999));

        // Step past arrival time
        for t in (launch_tick + 1)..=(launch_tick + 20) {
            world.step(&empty_bundle(t));
        }

        assert!(
            world.projectiles.is_empty(),
            "Projectile must have resolved"
        );
        assert_eq!(
            world.entities.get(&target).unwrap().hp,
            initial_hp,
            "Subcase (a): Cargo pickup mid-flight must nullify damage"
        );
    }

    // Subcase (b): Cloaking mid-flight without detector
    {
        let mut world = create_test_world(0x10B);
        let pulse_id = world.catalog.id_by_slug(pulse_slug).expect(pulse_slug);

        let shooter = world
            .spawn_unit(
                pulse_id,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
            )
            .expect("spawn shooter");
        let target = world
            .spawn_unit(
                pulse_id,
                PlayerId(1),
                SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(16)),
            )
            .expect("spawn target");

        let weapon = custom_projectile_weapon(
            SimScalar::from_f32_lossy(0.60),
            ProjectileTargetLossBehavior::Fizzle,
            DEFAULT_PROJECTILE_LIFETIME_TICKS,
            None,
        );
        let mut arch = world.catalog.get(pulse_id).unwrap().clone();
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
        world.catalog.register(arch);

        let initial_hp = world.entities.get(&target).unwrap().hp;
        world
            .entities
            .get_mut(&shooter)
            .unwrap()
            .issue_order(UnitOrder::AttackTarget { target }, false);

        let mut launch_tick = 1;
        for t in 1..=10 {
            world.step(&empty_bundle(t));
            if !world.projectiles.is_empty() {
                launch_tick = t;
                break;
            }
        }

        // Target cloaks mid-flight; shooter player has no detector vision
        world.entities.get_mut(&target).unwrap().is_cloaked = true;

        for t in (launch_tick + 1)..=(launch_tick + 20) {
            world.step(&empty_bundle(t));
        }

        assert!(
            world.projectiles.is_empty(),
            "Projectile must have resolved"
        );
        assert_eq!(
            world.entities.get(&target).unwrap().hp,
            initial_hp,
            "Subcase (b): Cloaking mid-flight without detector must nullify damage"
        );
    }

    // Subcase (c): Invulnerability mid-flight
    {
        let mut world = create_test_world(0x10C);
        let pulse_id = world.catalog.id_by_slug(pulse_slug).expect(pulse_slug);

        let shooter = world
            .spawn_unit(
                pulse_id,
                PlayerId(0),
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
            )
            .expect("spawn shooter");
        let target = world
            .spawn_unit(
                pulse_id,
                PlayerId(1),
                SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(16)),
            )
            .expect("spawn target");

        let weapon = custom_projectile_weapon(
            SimScalar::from_f32_lossy(0.60),
            ProjectileTargetLossBehavior::Fizzle,
            DEFAULT_PROJECTILE_LIFETIME_TICKS,
            None,
        );
        let mut arch = world.catalog.get(pulse_id).unwrap().clone();
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
        world.catalog.register(arch);

        let initial_hp = world.entities.get(&target).unwrap().hp;
        world
            .entities
            .get_mut(&shooter)
            .unwrap()
            .issue_order(UnitOrder::AttackTarget { target }, false);

        let mut launch_tick = 1;
        for t in 1..=10 {
            world.step(&empty_bundle(t));
            if !world.projectiles.is_empty() {
                launch_tick = t;
                break;
            }
        }

        // Target activates stasis / phase-shift / invulnerability mid-flight
        world.entities.get_mut(&target).unwrap().is_invulnerable = true;

        for t in (launch_tick + 1)..=(launch_tick + 20) {
            world.step(&empty_bundle(t));
        }

        assert!(
            world.projectiles.is_empty(),
            "Projectile must have resolved"
        );
        assert_eq!(
            world.entities.get(&target).unwrap().hp,
            initial_hp,
            "Subcase (c): Invulnerability mid-flight must nullify damage"
        );
    }
}

/// Test 2: Seeking missile / ordnance retargeting (`ProjectileTargetLossBehavior::RetargetNearest`).
/// When the primary target becomes unavailable in flight (e.g. loads into transport), the missile
/// queries the spatial neighborhood, locks onto a secondary enemy within `search_radius`, and damages it.
#[test]
fn test_seeking_projectile_retargets_nearest_available_enemy() {
    let mut world = create_test_world(0x200);
    let dummy_id = register_dummy_target(&mut world);
    let pulse_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Equip shooter with RetargetNearest weapon
    let search_radius = SimScalar::from_num(8);
    let weapon = custom_projectile_weapon(
        SimScalar::from_f32_lossy(0.60),
        ProjectileTargetLossBehavior::RetargetNearest { search_radius },
        DEFAULT_PROJECTILE_LIFETIME_TICKS,
        None,
    );
    let mut arch = world.catalog.get(pulse_id).unwrap().clone();
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon);
    world.catalog.register(arch);

    // Shooter (Blue) at (10.0, 16.0)
    let shooter = world
        .spawn_unit(
            pulse_id,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn shooter");

    // Primary target T1 (Red) at (16.0, 16.0)
    let target1 = world
        .spawn_unit(
            dummy_id,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(16)),
        )
        .expect("spawn target 1");

    // Secondary enemy target T2 (Red) at (17.0, 18.0) — within search_radius of 8.0
    let target2 = world
        .spawn_unit(
            dummy_id,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(17), SimScalar::from_num(18)),
        )
        .expect("spawn target 2");

    let t1_initial_hp = world.entities.get(&target1).unwrap().hp;
    let t2_initial_hp = world.entities.get(&target2).unwrap().hp;

    // Issue AttackTarget on T1
    world
        .entities
        .get_mut(&shooter)
        .unwrap()
        .issue_order(UnitOrder::AttackTarget { target: target1 }, false);

    // Step until projectile is in flight
    let mut launch_tick = 1;
    for t in 1..=10 {
        world.step(&empty_bundle(t));
        if !world.projectiles.is_empty() {
            launch_tick = t;
            break;
        }
    }
    assert_eq!(
        world.projectiles.len(),
        1,
        "Projectile must be in flight toward T1"
    );
    assert_eq!(world.projectiles[0].target_id, Some(target1));

    // Mid-flight, pick up T1 into transport (becomes unavailable!)
    world.load_unit_into_transport(target1, EntityId(999));

    // Step next tick: projectile should detect loss and dynamically retarget T2
    world.step(&empty_bundle(launch_tick + 1));
    assert_eq!(
        world.projectiles.len(),
        1,
        "Seeking projectile should remain in flight"
    );
    assert_eq!(
        world.projectiles[0].target_id,
        Some(target2),
        "Seeking missile must retarget secondary enemy T2 within search radius"
    );

    // Step until projectile impacts T2
    for t in (launch_tick + 2)..=(launch_tick + 30) {
        world.step(&empty_bundle(t));
        if world.projectiles.is_empty() {
            break;
        }
    }

    assert!(
        world.projectiles.is_empty(),
        "Seeking missile must resolve after hitting T2"
    );
    assert_eq!(
        world.entities.get(&target1).unwrap().hp,
        t1_initial_hp,
        "Primary target in transport must suffer 0 damage"
    );
    assert!(
        world.entities.get(&target2).unwrap().hp < t2_initial_hp,
        "Secondary target must take damage from the redirected seeking missile"
    );
}

/// Test 3: Missile timeout expiration (`remaining_lifetime_ticks`).
/// If a missile / seeking mine chases a target beyond its maximum lifetime (fuel limit),
/// it cleanly expires and despawns without dealing damage.
#[test]
fn test_missile_lifetime_timeout_expiration() {
    let mut world = create_test_world(0x300);
    let pulse_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Shooter at (10.0, 16.0)
    let shooter = world
        .spawn_unit(
            pulse_id,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn shooter");

    // Target at (25.0, 16.0) — distance 15.0 units away
    let target = world
        .spawn_unit(
            pulse_id,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(25), SimScalar::from_num(16)),
        )
        .expect("spawn target");

    // Speed 0.50 units/tick -> distance 15.0 requires ~30 ticks.
    // Set max lifetime to only 5 ticks!
    let weapon = custom_projectile_weapon(
        SimScalar::from_f32_lossy(0.50),
        ProjectileTargetLossBehavior::Fizzle,
        5, // Only 5 ticks of fuel!
        None,
    );
    let mut arch = world.catalog.get(pulse_id).unwrap().clone();
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon);
    world.catalog.register(arch);

    let initial_hp = world.entities.get(&target).unwrap().hp;
    world
        .entities
        .get_mut(&shooter)
        .unwrap()
        .issue_order(UnitOrder::AttackTarget { target }, false);

    // Step until missile launches
    let mut launch_tick = 1;
    for t in 1..=10 {
        world.step(&empty_bundle(t));
        if !world.projectiles.is_empty() {
            launch_tick = t;
            break;
        }
    }
    assert_eq!(world.projectiles.len(), 1, "Missile in flight");

    // Step 8 ticks (past the 5-tick lifetime limit)
    for t in (launch_tick + 1)..=(launch_tick + 8) {
        world.step(&empty_bundle(t));
    }

    // Assert missile fueled out and expired before reaching target
    assert!(
        world.projectiles.is_empty(),
        "Missile must expire and despawn once remaining_lifetime_ticks reaches 0"
    );
    assert_eq!(
        world.entities.get(&target).unwrap().hp,
        initial_hp,
        "Target out-distancing missile must suffer zero damage upon timeout"
    );
}

/// Test 4: `ProjectileTargetLossBehavior::ImpactLastKnownPosition` continues to ground coordinates
/// and applies splash damage to nearby bystanders even if the primary target vanished into cargo.
#[test]
fn test_ballistic_impact_last_known_position_splash() {
    let mut world = create_test_world(0x400);
    let dummy_id = register_dummy_target(&mut world);
    let pulse_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Ballistic weapon with ImpactLastKnownPosition and outer splash radius 2.0
    let splash = SplashSpec {
        inner_radius: SimScalar::ONE,
        middle_radius: SimScalar::from_f32_lossy(1.5),
        outer_radius: SimScalar::from_num(2),
        friendly_fire: false,
    };
    let weapon = custom_projectile_weapon(
        SimScalar::from_f32_lossy(0.60),
        ProjectileTargetLossBehavior::ImpactLastKnownPosition,
        DEFAULT_PROJECTILE_LIFETIME_TICKS,
        Some(splash),
    );
    let mut arch = world.catalog.get(pulse_id).unwrap().clone();
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon);
    world.catalog.register(arch);

    // Shooter at (10.0, 16.0)
    let shooter = world
        .spawn_unit(
            pulse_id,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16)),
        )
        .expect("spawn shooter");

    // Primary target T1 at (16.0, 16.0)
    let target1 = world
        .spawn_unit(
            dummy_id,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(16)),
        )
        .expect("spawn target 1");

    // Bystander enemy B at (16.5, 16.0) — within splash radius 2.0 of T1's position
    let bystander = world
        .spawn_unit(
            dummy_id,
            PlayerId(1),
            SimVec2::new(SimScalar::from_f32_lossy(16.5), SimScalar::from_num(16)),
        )
        .expect("spawn bystander");

    let bystander_initial_hp = world.entities.get(&bystander).unwrap().hp;

    world
        .entities
        .get_mut(&shooter)
        .unwrap()
        .issue_order(UnitOrder::AttackTarget { target: target1 }, false);

    let mut launch_tick = 1;
    for t in 1..=10 {
        world.step(&empty_bundle(t));
        if !world.projectiles.is_empty() {
            launch_tick = t;
            break;
        }
    }
    assert_eq!(world.projectiles.len(), 1, "Artillery projectile in flight");

    // Mid-flight, pick up primary target into transport
    world.load_unit_into_transport(target1, EntityId(999));

    // Step until projectile reaches destination
    for t in (launch_tick + 1)..=(launch_tick + 25) {
        world.step(&empty_bundle(t));
    }

    assert!(
        world.projectiles.is_empty(),
        "Artillery shell must detonate at ground target"
    );
    assert!(
        world.entities.get(&bystander).unwrap().hp < bystander_initial_hp,
        "Bystander unit standing near impact position must take splash damage from ImpactLastKnownPosition shell"
    );
}
