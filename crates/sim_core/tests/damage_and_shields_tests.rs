//! # Integration Test: SC2 Damage Math, Shields & 10s Regen Delay
//!
//! Verifies:
//! 1. Exact SC2 shield absorption, shield armor mitigation, and unmitigated HP spillover.
//! 2. Attribute bonus damage against Armored and Light targets.
//! 3. Multi-hit armor reduction (e.g. Zealot dual attack applying armor reduction twice).
//! 4. Minimum 0.5 HP damage clamping.
//! 5. Circular concentric splash falloff (100% / 50% / 25%).
//! 6. Shield and life regeneration wait for the archetype's catalog delays
//!    (`shield_regen_delay_ticks`, `hp_regen_delay_ticks`) after sustaining damage, for a
//!    shipped archetype and for scratch ones; any damage, to shields or life, restarts both.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::catalog::{AcquireLevel, AcquireTieBreak, PrimaryWeapon};
use sim_core::{
    calculate_hit_damage, calculate_splash_damage, SplashSpec, UnitArchetype, WeaponSpec,
    MIN_DAMAGE_CLAMP,
};
use sim_types::{
    ArchetypeCapabilities, AttributeFlags, FactionId, MoverType, ShapeKind, SilhouetteClass,
    SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitTypeId, WeaponVisualKind,
};
use sim_types::{Attribute, AttributeBonuses};

fn dummy_target_archetype(
    attributes: AttributeFlags,
    life_armor: SimScalar,
    shield_armor: SimScalar,
) -> UnitArchetype {
    UnitArchetype {
        unit_type_id: UnitTypeId(99),
        slug: "dummy_target".to_string(),
        name: "dummy_target".to_string(),
        faction: FactionId(0),
        display_name: "Dummy Target".to_string(),
        lore_description: "Target dummy for unit tests".to_string(),
        sc2_inspiration: None,
        showcase_hero: false,
        showcase_order: None,
        overclock: None,
        harvester: None,
        is_resource_dropoff: false,
        capabilities: ArchetypeCapabilities::default(),
        veo_prompts: std::collections::BTreeMap::new(),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::LightProngs,
        mover_type: MoverType::Ground,
        attributes,
        max_hp: SimScalar::from_num(100),
        max_shields: SimScalar::from_num(50),
        max_energy: SimScalar::ZERO,
        start_energy: SimScalar::ZERO,
        life_armor,
        shield_armor,
        hp_regen_per_tick: SimScalar::ZERO,
        hp_regen_delay_ticks: 0,
        shield_regen_per_tick: SimScalar::from_num(1),
        shield_regen_delay_ticks: Some(10),
        energy_regen_per_tick: SimScalar::ZERO,
        max_speed: SimScalar::ZERO,
        acceleration: SimScalar::ZERO,
        deceleration: SimScalar::ZERO,
        turn_rate: SimAngle::ZERO,
        turn_before_move: false,
        stationary_turn_rate: SimAngle::ZERO,
        collision_radius: SimScalar::HALF,
        separation_radius: SimScalar::HALF,
        push_priority: 10,
        sight_radius: SimScalar::from_num(8),
        scan_range: SimScalar::from_num(8),
        attack_target_priority: 20,
        always_a_threat: false,
        acquire_level: AcquireLevel::Offensive,
        primary_weapon: PrimaryWeapon::Either,
        acquire_tie_break: AcquireTieBreak::Closest,
        mineral_cost: 0,
        gas_cost: 0,
        supply_cost: 0,
        supply_provided: 0,
        build_time_ticks: 0,
        weapon: None,
        ground_weapon: None,
        air_weapon: None,
        abilities: Vec::new(),
        is_detector: false,
        requires_structures: Vec::new(),
        trains_units: Vec::new(),
        researches_upgrades: Vec::new(),
        armor_class: None,
        modes: Vec::new(),
    }
}

fn dummy_weapon_spec(
    base: SimScalar,
    bonus_armored: SimScalar,
    bonus_light: SimScalar,
) -> WeaponSpec {
    WeaponSpec {
        name: "TestGun".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(5),
        firing_arc: SimAngle::from_degrees(180),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        base_damage: base,
        attribute_bonuses: AttributeBonuses::NONE
            .with(Attribute::Armored, bonus_armored)
            .with(Attribute::Light, bonus_light),
        attacks_count: 1,
        strike_interval_ticks: 0,
        followup_max_distance: None,
        period_ticks: 20,
        damage_point_ticks: 2,
        backswing_ticks: 4,
        splash: None,
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
        suicide_on_attack: false,
    }
}

#[test]
fn test_sc2_shield_spillover_exact_formula() {
    // Target with 20 Shields (ShieldArmor = 1) and 100 HP (LifeArmor = 2)
    let arch = dummy_target_archetype(
        AttributeFlags::LIGHT | AttributeFlags::BIOLOGICAL,
        SimScalar::from_num(2),
        SimScalar::from_num(1),
    );
    let target_shields = SimScalar::from_num(20);
    let target_hp = SimScalar::from_num(100);

    // Strike with 30 raw damage
    let weapon = dummy_weapon_spec(SimScalar::from_num(30), SimScalar::ZERO, SimScalar::ZERO);
    let res = calculate_hit_damage(&weapon, &arch, target_shields, target_hp);

    // 1. Shield portion: 30 - 1 = 29 mitigated damage
    // 2. Shields absorb full 20 shields
    assert_eq!(res.shield_damage, SimScalar::from_num(20));

    // 3. Absorbed ratio = 20 / 29
    // Leftover raw = 30 * (1 - 20/29) = 30 * 9 / 29 = 270 / 29
    let leftover_raw = SimScalar::from_num(30)
        * (SimScalar::ONE - SimScalar::from_num(20) / SimScalar::from_num(29));
    let expected_life = (leftover_raw - SimScalar::from_num(2)).max(MIN_DAMAGE_CLAMP);

    assert_eq!(res.life_damage, expected_life);
    assert!(!res.killed);
}

#[test]
fn test_attribute_bonus_damage() {
    let marauder_gun = dummy_weapon_spec(
        SimScalar::from_num(10),
        SimScalar::from_num(10),
        SimScalar::ZERO,
    );

    let armored_target =
        dummy_target_archetype(AttributeFlags::ARMORED, SimScalar::ZERO, SimScalar::ZERO);
    let light_target =
        dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);

    let res_armored = calculate_hit_damage(
        &marauder_gun,
        &armored_target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(
        res_armored.life_damage,
        SimScalar::from_num(20),
        "Should deal 10 base + 10 armored bonus"
    );

    let res_light = calculate_hit_damage(
        &marauder_gun,
        &light_target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(
        res_light.life_damage,
        SimScalar::from_num(10),
        "Should deal only 10 base against Light"
    );
}

#[test]
fn test_multi_hit_zealot_armor_reduction() {
    let catalog = sim_fixtures::shipped_catalogs().units;
    let zealot = catalog
        .get_by_slug("psi_grey")
        .expect("Psi-Blade Grey exists");
    let weapon = zealot.weapon.as_ref().expect("Zealot weapon exists");

    assert_eq!(
        weapon.attacks_count, 2,
        "Zealot must have 2 attacks per cycle"
    );
    assert_eq!(weapon.base_damage, SimScalar::from_num(8));

    // Target with 2 life armor, 0 shields
    let target = dummy_target_archetype(
        AttributeFlags::ARMORED,
        SimScalar::from_num(2),
        SimScalar::ZERO,
    );

    let mut total_dmg = SimScalar::ZERO;
    let mut cur_hp = SimScalar::from_num(100);

    for _ in 0..weapon.attacks_count {
        let res = calculate_hit_damage(weapon, &target, SimScalar::ZERO, cur_hp);
        total_dmg += res.life_damage;
        cur_hp -= res.life_damage;
    }

    // Each hit is 8 - 2 = 6, total = 12 (instead of 16 - 2 = 14)
    assert_eq!(
        total_dmg,
        SimScalar::from_num(12),
        "Armor must be subtracted from each attack strike"
    );
}

#[test]
fn test_minimum_damage_clamp() {
    let weak_gun = dummy_weapon_spec(SimScalar::from_num(1), SimScalar::ZERO, SimScalar::ZERO);
    let high_armor_target = dummy_target_archetype(
        AttributeFlags::ARMORED,
        SimScalar::from_num(10),
        SimScalar::ZERO,
    );

    let res = calculate_hit_damage(
        &weak_gun,
        &high_armor_target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(
        res.life_damage,
        SimScalar::from_num(0.5),
        "Damage must be clamped to minimum 0.5 HP"
    );
}

#[test]
fn test_circular_splash_falloff() {
    let weapon = dummy_weapon_spec(SimScalar::from_num(40), SimScalar::ZERO, SimScalar::ZERO);
    let splash = SplashSpec {
        inner_radius: SimScalar::from_num(1),
        middle_radius: SimScalar::from_num(2),
        outer_radius: SimScalar::from_num(3),
        friendly_fire: false,
    };
    let target = dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);

    // Inner: 100% (40 damage)
    let res_inner = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(0.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(res_inner.expect("hit").life_damage, SimScalar::from_num(40));

    // Middle: 50% (20 damage)
    let res_mid = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(1.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(res_mid.expect("hit").life_damage, SimScalar::from_num(20));

    // Outer: 25% (10 damage)
    let res_outer = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(2.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert_eq!(res_outer.expect("hit").life_damage, SimScalar::from_num(10));

    // Miss: > outer_radius (None)
    let res_miss = calculate_splash_damage(
        &weapon,
        &splash,
        SimScalar::from_num(3.5),
        &target,
        SimScalar::ZERO,
        SimScalar::from_num(100),
    );
    assert!(res_miss.is_none());
}

/// Damages a fresh unit's shields by `damage`, steps the world `delay - 1` empty ticks
/// asserting that the shields hold, then one more and asserts that they started regenerating.
/// The delay is the archetype's `shield_regen_delay_ticks`, counted in ticks (25 per second)
/// after the damage.
fn assert_shields_regenerate_after_catalog_delay(
    world: &mut sim_core::SimWorld,
    unit_type: UnitTypeId,
    damage: SimScalar,
) {
    use sim_types::PlayerId;

    let delay = world
        .catalog
        .get(unit_type)
        .expect("archetype registered")
        .shield_regen_delay_ticks
        .expect("a shielded archetype carries a regeneration delay");
    assert!(delay > 1, "the test needs at least one waiting tick");
    let unit_id = world
        .spawn_unit(
            unit_type,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn shielded unit");

    let damaged = {
        let unit = world.entities.get_mut(&unit_id).expect("unit exists");
        let before = unit.shields;
        unit.apply_damage(damage, SimScalar::ZERO, SimAngle::ZERO);
        assert_eq!(unit.shields, before - damage);
        assert_eq!(unit.ticks_since_last_damage, 0);
        unit.shields
    };

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    for tick in 1..u32::from(delay) {
        world.step(&empty_bundle(tick));
        let unit = world.entities.get(&unit_id).expect("unit exists");
        assert_eq!(
            unit.shields, damaged,
            "Shields must not regenerate before {delay} ticks without damage (tick {tick})"
        );
    }

    world.step(&empty_bundle(u32::from(delay)));
    let unit = world.entities.get(&unit_id).expect("unit exists");
    assert!(
        unit.shields > damaged,
        "Shields must begin regenerating on tick {delay} after the damage"
    );
}

#[test]
fn test_shield_regeneration_waits_for_the_catalog_delay() {
    use sim_map::{CliffLevel, GameMap, MapCell};

    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x5555_AAAA);

    let psi_grey_id = world
        .catalog
        .id_by_slug("psi_grey")
        .expect("psi_grey in catalog");
    // Damage the zealot's shields from 50 down to 20.
    assert_shields_regenerate_after_catalog_delay(&mut world, psi_grey_id, SimScalar::from_num(30));
}

#[test]
fn a_scratch_archetype_with_a_10_tick_delay_regenerates_from_the_10th_tick_after_damage() {
    use sim_map::{CliffLevel, GameMap, MapCell};

    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x5555_AAAA);

    let mut scratch =
        dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);
    let free_id = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("a free unit type id");
    scratch.unit_type_id = free_id;
    scratch.slug = "scratch_shield_delay".to_string();
    scratch.name = scratch.slug.clone();
    scratch.shield_regen_delay_ticks = Some(10);
    world.catalog.register(scratch);

    assert_shields_regenerate_after_catalog_delay(&mut world, free_id, SimScalar::from_num(30));
}

/// An empty tick bundle for `tick`.
fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// A 64 × 64 flat world on the shipped catalog.
fn flat_world(seed: u64) -> sim_core::SimWorld {
    use sim_map::{CliffLevel, GameMap, MapCell};

    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    sim_fixtures::world(map, seed)
}

/// Registers a scratch unit archetype under the first free unit type id: 100 life,
/// regenerating 1 life per tick after `life_delay` ticks without damage. With `shields` it
/// keeps the dummy's 50 shields (1 per tick after 10 ticks); without, it has none.
fn register_life_regenerating_scratch(
    world: &mut sim_core::SimWorld,
    slug: &str,
    life_delay: u16,
    shields: bool,
) -> UnitTypeId {
    let mut scratch =
        dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);
    let free_id = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("a free unit type id");
    scratch.unit_type_id = free_id;
    scratch.slug = slug.to_string();
    scratch.name = slug.to_string();
    scratch.hp_regen_per_tick = SimScalar::ONE;
    scratch.hp_regen_delay_ticks = life_delay;
    if !shields {
        scratch.max_shields = SimScalar::ZERO;
        scratch.shield_regen_per_tick = SimScalar::ZERO;
        scratch.shield_regen_delay_ticks = None;
    }
    world.catalog.register(scratch);
    free_id
}

/// Spawns one unit of `unit_type` for player 0 at (10, 10) and returns its id.
fn spawn_one(world: &mut sim_core::SimWorld, unit_type: UnitTypeId) -> sim_types::EntityId {
    world
        .spawn_unit(
            unit_type,
            sim_types::PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn unit")
}

/// Steps the world through each tick of `ticks`, asserting after each that the unit's life is
/// still `held`.
fn assert_life_holds(
    world: &mut sim_core::SimWorld,
    unit_id: sim_types::EntityId,
    held: SimScalar,
    ticks: std::ops::RangeInclusive<u32>,
    why: &str,
) {
    for tick in ticks {
        world.step(&empty_bundle(tick));
        let unit = world.entities.get(&unit_id).expect("unit exists");
        assert_eq!(unit.hp, held, "{why}: life must hold on tick {tick}");
    }
}

/// Damages a fresh unit's life by `life_damage` through `Entity::apply_damage`, steps the
/// world `delay - 1` empty ticks asserting that life holds, then one more and asserts that it
/// rose by exactly the archetype's `hp_regen_per_tick`. The delay is the archetype's
/// `hp_regen_delay_ticks`, counted in ticks (25 per Faster second) after the damage. Returns
/// the life gained on that first regenerating tick.
fn assert_life_regenerates_after_catalog_delay(
    world: &mut sim_core::SimWorld,
    unit_type: UnitTypeId,
    life_damage: SimScalar,
) -> SimScalar {
    let (delay, rate) = {
        let arch = world.catalog.get(unit_type).expect("archetype registered");
        (arch.hp_regen_delay_ticks, arch.hp_regen_per_tick)
    };
    assert!(delay > 1, "the test needs at least one waiting tick");
    assert!(rate > SimScalar::ZERO, "the archetype must regenerate life");
    let unit_id = spawn_one(world, unit_type);
    let damaged = {
        let unit = world.entities.get_mut(&unit_id).expect("unit exists");
        let before = unit.hp;
        unit.apply_damage(SimScalar::ZERO, life_damage, SimAngle::ZERO);
        assert_eq!(unit.hp, before - life_damage);
        assert_eq!(unit.ticks_since_last_damage, 0);
        unit.hp
    };
    assert_life_holds(
        world,
        unit_id,
        damaged,
        1..=u32::from(delay) - 1,
        "Life must not regenerate before the catalog delay",
    );
    world.step(&empty_bundle(u32::from(delay)));
    let unit = world.entities.get(&unit_id).expect("unit exists");
    assert_eq!(
        unit.hp,
        damaged + rate,
        "Life must regenerate by its rate on tick {delay} after the damage"
    );
    unit.hp - damaged
}

#[test]
fn a_scratch_archetype_with_a_10_tick_life_delay_regenerates_from_the_10th_tick_after_damage() {
    let mut world = flat_world(0x5555_AAAA);
    let scratch = register_life_regenerating_scratch(&mut world, "scratch_life_delay", 10, false);
    let gained =
        assert_life_regenerates_after_catalog_delay(&mut world, scratch, SimScalar::from_num(20));
    assert_eq!(gained, SimScalar::ONE);
}

#[test]
fn test_life_regeneration_waits_for_the_catalog_delay() {
    let mut world = flat_world(0x5555_AAAA);
    // Found by property, not by slug: the shipped archetype with the longest life delay.
    let (unit_type, delay) = world
        .catalog
        .archetypes()
        .map(|arch| (arch.unit_type_id, arch.hp_regen_delay_ticks))
        .max_by_key(|&(id, delay)| (delay, std::cmp::Reverse(id)))
        .expect("the shipped catalog is not empty");
    assert!(
        delay > 0,
        "some shipped archetype must wait before regenerating life"
    );
    let gained =
        assert_life_regenerates_after_catalog_delay(&mut world, unit_type, SimScalar::from_num(20));
    let rate = world
        .catalog
        .get(unit_type)
        .expect("archetype registered")
        .hp_regen_per_tick;
    assert_eq!(gained, rate);
}

#[test]
fn life_regeneration_without_a_delay_runs_on_the_tick_after_damage() {
    let mut world = flat_world(0x5555_AAAA);
    let scratch = register_life_regenerating_scratch(&mut world, "scratch_life_no_delay", 0, false);
    let unit_id = spawn_one(&mut world, scratch);
    let damaged = {
        let unit = world.entities.get_mut(&unit_id).expect("unit exists");
        unit.apply_damage(SimScalar::ZERO, SimScalar::from_num(20), SimAngle::ZERO);
        unit.hp
    };
    world.step(&empty_bundle(1));
    let unit = world.entities.get(&unit_id).expect("unit exists");
    assert_eq!(
        unit.hp,
        damaged + SimScalar::ONE,
        "A delay of 0 regenerates on the first tick after the damage"
    );
}

#[test]
fn shield_damage_alone_restarts_the_life_regeneration_delay() {
    let mut world = flat_world(0x5555_AAAA);
    let scratch = register_life_regenerating_scratch(&mut world, "scratch_shared_delay", 10, true);
    let unit_id = spawn_one(&mut world, scratch);
    let damaged = {
        let unit = world.entities.get_mut(&unit_id).expect("unit exists");
        unit.apply_damage(SimScalar::ZERO, SimScalar::from_num(20), SimAngle::ZERO);
        unit.hp
    };
    assert_life_holds(
        &mut world,
        unit_id,
        damaged,
        1..=5,
        "Life damage starts the delay",
    );
    {
        let unit = world.entities.get_mut(&unit_id).expect("unit exists");
        unit.apply_damage(SimScalar::from_num(10), SimScalar::ZERO, SimAngle::ZERO);
        assert_eq!(unit.hp, damaged, "the shield hit must not touch life");
    }
    assert_life_holds(
        &mut world,
        unit_id,
        damaged,
        6..=14,
        "A shield-only hit restarts the life delay",
    );
    world.step(&empty_bundle(15));
    let unit = world.entities.get(&unit_id).expect("unit exists");
    assert_eq!(
        unit.hp,
        damaged + SimScalar::ONE,
        "Life must regenerate on the 10th tick after the shield hit"
    );
}

#[test]
fn a_unit_never_damaged_regenerates_life_at_once() {
    let mut world = flat_world(0x5555_AAAA);
    let scratch = register_life_regenerating_scratch(&mut world, "scratch_never_hit", 10, false);
    let unit_id = spawn_one(&mut world, scratch);
    let lowered = {
        let unit = world.entities.get_mut(&unit_id).expect("unit exists");
        assert_eq!(unit.ticks_since_last_damage, u16::MAX);
        unit.hp -= SimScalar::from_num(20);
        unit.hp
    };
    world.step(&empty_bundle(1));
    let unit = world.entities.get(&unit_id).expect("unit exists");
    assert_eq!(
        unit.hp,
        lowered + SimScalar::ONE,
        "A unit never damaged satisfies any delay"
    );
}

/// Test 7: Verify that splash with friendly_fire == true damages nearby allied units,
/// while friendly_fire == false splash protects nearby allied units from harm.
#[test]
fn test_friendly_fire_splash_damages_allies_vs_enemy_only_splash() {
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{PlayerId, UnitOrder};

    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x9999);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Case A: Weapon with friendly_fire: true
    let friendly_fire_weapon = WeaponSpec {
        name: "Artillery Shell (Friendly Fire)".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(10),
        firing_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        base_damage: SimScalar::from_num(40),
        attribute_bonuses: AttributeBonuses::NONE,
        attacks_count: 1,
        strike_interval_ticks: 0,
        followup_max_distance: None,
        period_ticks: 30,
        damage_point_ticks: 1,
        backswing_ticks: 2,
        splash: Some(SplashSpec {
            inner_radius: SimScalar::ONE,
            middle_radius: SimScalar::from_num(1.5),
            outer_radius: SimScalar::from_num(2.0),
            friendly_fire: true,
        }),
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
        suicide_on_attack: false,
    };

    // Case B: Weapon with friendly_fire: false
    let enemy_only_weapon = WeaponSpec {
        name: "Disruptor Shell (Enemy Only)".to_string(),
        splash: Some(SplashSpec {
            inner_radius: SimScalar::ONE,
            middle_radius: SimScalar::from_num(1.5),
            outer_radius: SimScalar::from_num(2.0),
            friendly_fire: false,
        }),
        ..friendly_fire_weapon.clone()
    };

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    let mut dummy_arch =
        dummy_target_archetype(AttributeFlags::LIGHT, SimScalar::ZERO, SimScalar::ZERO);
    dummy_arch.max_shields = SimScalar::ZERO;
    world.catalog.register(dummy_arch);
    let dummy_id = UnitTypeId(99);

    // 1. Test friendly_fire: true
    {
        // Attacker (Blue) at (10.0, 15.0)
        let attacker = world
            .spawn_unit(
                pulse_bot_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(15)),
            )
            .expect("spawn attacker");
        // Primary target (Red) at (15.0, 15.0)
        let target_red = world
            .spawn_unit(
                dummy_id,
                player_red,
                SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(15)),
            )
            .expect("spawn red target");
        // Ally (Blue) at (15.5, 15.0) - within splash inner radius (dist = 0.5 <= 1.0)
        let ally_blue = world
            .spawn_unit(
                dummy_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(15.5), SimScalar::from_num(15)),
            )
            .expect("spawn blue ally");

        // Give attacker the friendly_fire weapon
        world.catalog.get_mut(pulse_bot_id).unwrap().ground_weapon = Some(friendly_fire_weapon);

        world
            .entities
            .get_mut(&attacker)
            .unwrap()
            .issue_order(UnitOrder::attack(target_red), false)
            .expect("valid authored fixture order or buff");

        let red_hp_before = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_before = world.entities.get(&ally_blue).unwrap().hp;

        for tick in 1..=10 {
            world.step(&empty_bundle(tick));
            if world.entities.get(&target_red).unwrap().hp < red_hp_before {
                break;
            }
        }

        let red_hp_after = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_after = world.entities.get(&ally_blue).unwrap().hp;

        assert!(
            red_hp_after < red_hp_before,
            "Primary red target must take direct hit damage"
        );
        assert!(
            ally_hp_after < ally_hp_before,
            "Blue ally must sustain splash damage when weapon friendly_fire == true"
        );

        // Retire this fixture through the actual lifecycle so captured supply
        // is released before the next independent splash comparison.
        for entity in world.entities.values_mut() {
            entity.hp = SimScalar::ZERO;
        }
        world.reap_dead_and_emit_shatter();
        assert!(world.entities.is_empty());
    }

    // 2. Test friendly_fire: false
    {
        // Attacker (Blue) at (10.0, 15.0)
        let attacker = world
            .spawn_unit(
                pulse_bot_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(15)),
            )
            .expect("spawn attacker 2");
        // Primary target (Red) at (15.0, 15.0)
        let target_red = world
            .spawn_unit(
                dummy_id,
                player_red,
                SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(15)),
            )
            .expect("spawn red target 2");
        // Ally (Blue) at (15.5, 15.0) - within splash inner radius (dist = 0.5 <= 1.0)
        let ally_blue = world
            .spawn_unit(
                dummy_id,
                player_blue,
                SimVec2::new(SimScalar::from_num(15.5), SimScalar::from_num(15)),
            )
            .expect("spawn blue ally 2");

        // Give attacker the enemy-only splash weapon
        world.catalog.get_mut(pulse_bot_id).unwrap().ground_weapon = Some(enemy_only_weapon);

        world
            .entities
            .get_mut(&attacker)
            .unwrap()
            .issue_order(UnitOrder::attack(target_red), false)
            .expect("valid authored fixture order or buff");

        let red_hp_before = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_before = world.entities.get(&ally_blue).unwrap().hp;

        for tick in 11..=20 {
            world.step(&empty_bundle(tick));
            if world.entities.get(&target_red).unwrap().hp < red_hp_before {
                break;
            }
        }

        let red_hp_after = world.entities.get(&target_red).unwrap().hp;
        let ally_hp_after = world.entities.get(&ally_blue).unwrap().hp;

        assert!(
            red_hp_after < red_hp_before,
            "Primary red target must take direct hit damage"
        );
        assert_eq!(
            ally_hp_after, ally_hp_before,
            "Blue ally must remain completely unharmed when weapon friendly_fire == false"
        );
    }
}

/// S-13: a unit-specific upgrade bonus falls off with splash distance like all other damage.
#[test]
fn s13_splash_falloff_scales_stat_bonus_damage() {
    use sim_core::calculate_splash_damage_upgraded;
    use sim_types::{PlayerUpgradeState, UnitStatDeltas};

    let weapon = dummy_weapon_spec(SimScalar::from_num(20), SimScalar::ZERO, SimScalar::ZERO);
    let target = dummy_target_archetype(AttributeFlags::empty(), SimScalar::ZERO, SimScalar::ZERO);
    let splash = SplashSpec {
        inner_radius: SimScalar::ONE,
        middle_radius: SimScalar::from_num(2),
        outer_radius: SimScalar::from_num(4),
        friendly_fire: false,
    };
    let mut attacker = PlayerUpgradeState::default();
    attacker.unit_stat_modifiers.insert(
        "shooter".to_owned(),
        UnitStatDeltas {
            bonus_damage_delta: SimScalar::from_num(8),
            ..UnitStatDeltas::default()
        },
    );
    let res = calculate_splash_damage_upgraded(
        &weapon,
        Some("shooter"),
        &attacker,
        &splash,
        SimScalar::from_num(3),
        &target,
        &PlayerUpgradeState::default(),
        SimScalar::ZERO,
        SimScalar::from_num(100),
    )
    .expect("inside the outer radius");
    assert_eq!(res.life_damage, SimScalar::from_num(7), "(20 + 8) at 25%");
}

/// S-14: per-level attribute bonus follows the attribute the weapon actually has a bonus for.
#[test]
fn s14_bonus_upgrade_follows_the_weapons_attribute_bonus() {
    use sim_core::calculate_hit_damage_upgraded;
    use sim_types::{PlayerUpgradeState, WeaponClassId};

    let mut upgrades = PlayerUpgradeState::default();
    upgrades.weapon_levels.insert(WeaponClassId(1), 2);
    let mut weapon = dummy_weapon_spec(
        SimScalar::from_num(10),
        SimScalar::ZERO,
        SimScalar::from_num(5),
    );
    weapon.bonus_upgrade_per_level = SimScalar::ONE;
    weapon.weapon_class = Some(WeaponClassId(1));

    let damage = |attributes| {
        let target = dummy_target_archetype(attributes, SimScalar::ZERO, SimScalar::ZERO);
        calculate_hit_damage_upgraded(
            &weapon,
            None,
            &upgrades,
            &target,
            &PlayerUpgradeState::default(),
            SimScalar::ZERO,
            SimScalar::from_num(100),
        )
        .life_damage
    };
    assert_eq!(damage(AttributeFlags::LIGHT), SimScalar::from_num(17));
    assert_eq!(
        damage(AttributeFlags::ARMORED),
        SimScalar::from_num(10),
        "no armored bonus means no armored per-level growth"
    );
    assert_eq!(
        damage(AttributeFlags::LIGHT | AttributeFlags::ARMORED),
        SimScalar::from_num(17),
        "only the Light bonus applies, so its per-level bonus is granted once"
    );
}

/// B-14 (plan `2026-11-05-sc2-deviation-fixes` WP6): every attribute bonus that applies gains the
/// per-level upgrade, as each `AttributeBonus` entry of a `StarCraft` II damage effect is upgraded
/// on its own (`liberty` UpgradeData:679, `ParasiteSporeDamage` `AttributeBonus[Massive]`).
#[test]
fn b14_each_applying_attribute_bonus_gains_its_upgrade() {
    use sim_core::calculate_hit_damage_upgraded;
    use sim_types::{PlayerUpgradeState, WeaponClassId};

    let mut upgrades = PlayerUpgradeState::default();
    upgrades.weapon_levels.insert(WeaponClassId(1), 2);
    let mut weapon = dummy_weapon_spec(
        SimScalar::from_num(10),
        SimScalar::from_num(3),
        SimScalar::from_num(5),
    );
    weapon.bonus_upgrade_per_level = SimScalar::ONE;
    weapon.weapon_class = Some(WeaponClassId(1));
    let target = dummy_target_archetype(
        AttributeFlags::LIGHT | AttributeFlags::ARMORED,
        SimScalar::ZERO,
        SimScalar::ZERO,
    );
    let damage = calculate_hit_damage_upgraded(
        &weapon,
        None,
        &upgrades,
        &target,
        &PlayerUpgradeState::default(),
        SimScalar::ZERO,
        SimScalar::from_num(100),
    )
    .life_damage;
    assert_eq!(damage, SimScalar::from_num(22), "10 + (3 + 2) + (5 + 2)");
}
