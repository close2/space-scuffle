//! # Integration Test: the damage-absorbing barrier (B-41)
//!
//! `StarCraft` II's Immortal Barrier as a capability any archetype may carry
//! (`ArchetypeCapabilities::barrier`): the first damage while the barrier is ready raises it
//! for `duration_ticks` and starts `cooldown_ticks`; while raised it absorbs damage before
//! armour until `absorb_limit` hit points are absorbed (`voidmulti` BehaviorData:273-279,
//! :577-609; `void` BehaviorData:97-108). These tests drive real attacks, splash and storm
//! through `SimWorld::step` on scratch archetypes cloned from a shipped unit.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

use sim_core::{SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttributeBonuses, AttributeFlags, BarrierSpec, EntityId, PlayerId, SimAngle, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId, WeaponVisualKind,
};

/// A 64 × 64 flat world on the shipped catalog.
fn flat_world() -> SimWorld {
    let cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, cell).expect("invariant: valid map");
    sim_fixtures::world(map, 0x0b41)
}

/// A hitscan weapon dealing `damage` every `period` ticks at range 5, without random delay.
fn weapon(damage: i32, period: u16) -> WeaponSpec {
    WeaponSpec {
        name: "BarrierTestGun".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(5),
        firing_arc: SimAngle::from_degrees(360),
        acquire_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        turret_idle: sim_types::TurretIdle::Hold,
        base_damage: SimScalar::from_num(damage),
        attribute_bonuses: AttributeBonuses::NONE,
        attacks_count: 1,
        strike_interval_ticks: 0,
        followup_max_distance: None,
        channeled: false,
        period_ticks: period,
        damage_point_ticks: 1,
        backswing_ticks: 1,
        random_delay_min: SimScalar::ZERO,
        random_delay_max: SimScalar::ZERO,
        splash: None,
        projectile_speed: None,
        weapon_class: None,
        upgrade_bonus_per_level: SimScalar::ZERO,
        bonus_upgrade_per_level: SimScalar::ZERO,
        target_loss_behavior: sim_types::ProjectileTargetLossBehavior::Fizzle,
        max_projectile_lifetime_ticks: sim_types::DEFAULT_PROJECTILE_LIFETIME_TICKS,
        suicide_on_attack: false,
        structure_strike: None,
    }
}

/// Registers a clone of the first shipped armed unit under a free id, changed by `edit`.
fn register(world: &mut SimWorld, slug: &str, edit: impl FnOnce(&mut UnitArchetype)) -> UnitTypeId {
    let base = world
        .catalog
        .archetypes()
        .find(|arch| arch.weapon.is_some() && arch.max_speed > SimScalar::ZERO)
        .expect("a shipped armed unit")
        .clone();
    let free = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("a free unit type id");
    let mut arch = base;
    arch.unit_type_id = free;
    arch.slug = slug.to_string();
    arch.name = slug.to_string();
    edit(&mut arch);
    world.catalog.register(arch);
    free
}

/// A passive target with 1000 life, no shields or regeneration, `life_armor`, and `barrier`.
fn register_target(
    world: &mut SimWorld,
    life_armor: i32,
    barrier: Option<BarrierSpec>,
) -> UnitTypeId {
    register(world, "barrier_test_target", |arch| {
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.max_hp = SimScalar::from_num(1000);
        arch.max_shields = SimScalar::ZERO;
        arch.shield_regen_per_tick = SimScalar::ZERO;
        arch.shield_regen_delay_ticks = None;
        arch.hp_regen_per_tick = SimScalar::ZERO;
        arch.hp_regen_delay_ticks = 0;
        arch.life_armor = SimScalar::from_num(life_armor);
        arch.capabilities.barrier = barrier;
    })
}

/// An attacker armed with `gun` and nothing else.
fn register_attacker(world: &mut SimWorld, gun: WeaponSpec) -> UnitTypeId {
    register(world, "barrier_test_attacker", |arch| {
        arch.weapon = Some(gun);
        arch.ground_weapon = None;
        arch.air_weapon = None;
    })
}

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn empty(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Spawns `attacker` at (10, 10) and `target` at (13, 10) holding position, orders the attack, steps `ticks`
/// ticks and returns the life the target lost on each tick it lost any, in order.
fn life_losses(
    world: &mut SimWorld,
    attacker: UnitTypeId,
    target: UnitTypeId,
    ticks: u32,
) -> (EntityId, Vec<(u32, SimScalar)>) {
    let shooter = world
        .spawn_unit(attacker, PlayerId(0), at(10, 10))
        .expect("spawn attacker");
    let victim = world
        .spawn_unit(target, PlayerId(1), at(13, 10))
        .expect("spawn target");
    world
        .entities
        .get_mut(&shooter)
        .expect("attacker exists")
        .issue_order(UnitOrder::attack(victim), false)
        .expect("attack order admitted");
    // Holding, so the unarmed target neither flees nor is pushed out of range.
    world
        .entities
        .get_mut(&victim)
        .expect("target exists")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    let mut losses = Vec::new();
    let mut hp = world.entities[&victim].hp;
    for tick in 0..ticks {
        world.step(&empty(tick));
        let now = world.entities[&victim].hp;
        if now != hp {
            losses.push((tick, hp - now));
            hp = now;
        }
    }
    (victim, losses)
}

/// The ticks the attacker's weapon released on, read from its cooldown restarts.
fn shots(world: &mut SimWorld, attacker: UnitTypeId, target: UnitTypeId, ticks: u32) -> Vec<u32> {
    let (_, losses) = life_losses(world, attacker, target, ticks);
    losses.into_iter().map(|(tick, _)| tick).collect()
}

fn barrier(limit: i32, duration: u16, cooldown: u16) -> Option<BarrierSpec> {
    Some(BarrierSpec {
        absorb_limit: SimScalar::from_num(limit),
        duration_ticks: duration,
        cooldown_ticks: cooldown,
    })
}

fn n(value: i32) -> SimScalar {
    SimScalar::from_num(value)
}

#[test]
fn b41_the_barrier_absorbs_its_limit_then_life_takes_the_rest() {
    let mut world = flat_world();
    let attacker = register_attacker(&mut world, weapon(30, 10));
    let target = register_target(&mut world, 0, barrier(100, 54, 804));
    let (victim, losses) = life_losses(&mut world, attacker, target, 80);
    // Hits 1-3 (90) are absorbed whole; hit 4 has 10 absorbed and 20 reaches life.
    let amounts: Vec<SimScalar> = losses.iter().map(|&(_, loss)| loss).collect();
    assert!(amounts.len() >= 3, "{losses:?}");
    assert_eq!(amounts[0], n(20), "{losses:?}");
    assert!(amounts[1..].iter().all(|&loss| loss == n(30)), "{losses:?}");
    let entity = &world.entities[&victim];
    assert!(!entity.barrier.is_raised(), "a spent barrier is down");
    assert!(entity.barrier.cooldown_ticks > 0, "and cooling down");
}

#[test]
fn b41_without_the_capability_every_hit_lands() {
    let mut world = flat_world();
    let attacker = register_attacker(&mut world, weapon(30, 10));
    let target = register_target(&mut world, 0, None);
    let (_, losses) = life_losses(&mut world, attacker, target, 80);
    assert!(losses.len() >= 7, "{losses:?}");
    assert!(losses.iter().all(|&(_, loss)| loss == n(30)), "{losses:?}");
}

#[test]
fn b41_the_barrier_goes_down_when_its_window_ends() {
    // 5 a hit every 10 ticks never reaches the limit; the window of 54 ticks counted from the
    // raising hit covers the hits 0, 10, ..., 50 ticks after it, and the next one lands.
    let mut world = flat_world();
    let attacker = register_attacker(&mut world, weapon(5, 10));
    let plain = register_target(&mut world, 0, None);
    let released = shots(&mut world, attacker, plain, 120);
    assert!(released.len() >= 8, "{released:?}");
    let first = released[0];
    let expected_first_landing = released
        .iter()
        .copied()
        .find(|&tick| tick >= first + 54)
        .expect("a hit after the window");

    let mut world = flat_world();
    let attacker = register_attacker(&mut world, weapon(5, 10));
    let target = register_target(&mut world, 0, barrier(100, 54, 804));
    let (_, losses) = life_losses(&mut world, attacker, target, 120);
    assert_eq!(
        losses.first().map(|&(tick, _)| tick),
        Some(expected_first_landing),
        "{losses:?}; shots {released:?}"
    );
    assert!(losses.iter().all(|&(_, loss)| loss == n(5)), "{losses:?}");
}

#[test]
fn b41_the_barrier_rises_again_once_its_cooldown_ends() {
    // A 5-tick window holds one hit of a 10-tick gun; with a 30-tick cooldown the hit three
    // shots later raises the barrier again, and so on.
    let mut world = flat_world();
    let attacker = register_attacker(&mut world, weapon(30, 10));
    let plain = register_target(&mut world, 0, None);
    let released = shots(&mut world, attacker, plain, 70);
    assert!(released.len() >= 6, "{released:?}");
    // Each hit at or after the end of the last cooldown raises the barrier and is absorbed.
    let mut ready_at = 0;
    let mut absorbed = Vec::new();
    for &tick in &released {
        if tick >= ready_at {
            absorbed.push(tick);
            ready_at = tick + 30;
        }
    }
    assert!(absorbed.len() >= 2, "{released:?}");

    let mut world = flat_world();
    let attacker = register_attacker(&mut world, weapon(30, 10));
    let target = register_target(&mut world, 0, barrier(100, 5, 30));
    let (_, losses) = life_losses(&mut world, attacker, target, 70);
    let landed: Vec<u32> = losses.iter().map(|&(tick, _)| tick).collect();
    let expected: Vec<u32> = released
        .iter()
        .copied()
        .filter(|tick| !absorbed.contains(tick))
        .collect();
    assert_eq!(landed, expected, "absorbed {absorbed:?}");
}

#[test]
fn b41_the_barrier_absorbs_before_armour() {
    // Life armour 2, limit 50, 30 a hit: the first hit is absorbed whole (30 of the 50), the
    // second has 20 absorbed and 10 - 2 = 8 reaches life. Absorbing after armour would count
    // 28 against the limit and let 28 - 22 = 6 through.
    let mut world = flat_world();
    let attacker = register_attacker(&mut world, weapon(30, 10));
    let target = register_target(&mut world, 2, barrier(50, 54, 804));
    let (_, losses) = life_losses(&mut world, attacker, target, 40);
    assert_eq!(
        losses.first().map(|&(_, loss)| loss),
        Some(n(8)),
        "{losses:?}"
    );
    assert!(
        losses[1..].iter().all(|&(_, loss)| loss == n(28)),
        "{losses:?}"
    );
}

#[test]
fn b41_the_barrier_absorbs_splash() {
    // The primary target has no barrier; a barrier unit one cell beside it takes the full
    // splash of the first two shots without losing life, and the third reaches it.
    let mut world = flat_world();
    let mut gun = weapon(30, 10);
    gun.splash = Some(SplashSpec {
        zones: vec![sim_types::SplashZone {
            radius: n(2),
            fraction: SimScalar::ONE,
        }],
        shape: sim_types::SplashShape::Circle,
        friendly_fire: false,
    });
    let attacker = register_attacker(&mut world, gun);
    let plain = register_target(&mut world, 0, None);
    let shielded = register_target(&mut world, 0, barrier(60, 54, 804));
    let neighbour = world
        .spawn_unit(shielded, PlayerId(1), at(13, 11))
        .expect("spawn neighbour");
    world
        .entities
        .get_mut(&neighbour)
        .expect("neighbour exists")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    let start = world.entities[&neighbour].hp;
    let (_, primary_losses) = life_losses(&mut world, attacker, plain, 40);
    assert!(primary_losses.len() >= 3, "{primary_losses:?}");
    let entity = &world.entities[&neighbour];
    assert_eq!(
        start - entity.hp,
        n(30 * (i32::try_from(primary_losses.len()).expect("few losses") - 2))
    );
}

#[test]
fn b41_the_barrier_absorbs_storm_damage() {
    use sim_core::ActiveHazardZone;
    let mut world = flat_world();
    let target = register_target(&mut world, 0, barrier(100, 54, 804));
    let victim = world
        .spawn_unit(target, PlayerId(1), at(13, 10))
        .expect("spawn target");
    // Holding, so it stays in the storm instead of evacuating.
    world
        .entities
        .get_mut(&victim)
        .expect("target exists")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    world.active_hazards.push(ActiveHazardZone {
        id: 0,
        owner: PlayerId(0),
        center: at(13, 10),
        radius: n(2),
        delay_ticks: 0,
        remaining_ticks: 40,
        interval_ticks: 10,
        ticks_until_next_interval: 0,
        damage_per_interval: n(40),
        targets_friendly: false,
    });
    let start = world.entities[&victim].hp;
    let mut losses = Vec::new();
    let mut hp = start;
    for tick in 0..40 {
        world.step(&empty(tick));
        let now = world.entities[&victim].hp;
        if now != hp {
            losses.push(hp - now);
            hp = now;
        }
    }
    // Pulses of 40: two absorbed whole, the third has 20 absorbed, the fourth lands whole.
    assert_eq!(losses, vec![n(20), n(40)], "{losses:?}");
}

#[test]
fn b41_the_shipped_immortal_analogue_carries_the_sourced_barrier() {
    // 100 absorbed (`voidmulti` BehaviorData:278), 3 Normal s raised (:601) and 45 Normal s of
    // cooldown (:585), at 1.4 Normal s per Faster second and 25 ticks per Faster second:
    // 3 × 25 ÷ 1.4 = 53.6 → 54 ticks, 45 × 25 ÷ 1.4 = 803.6 → 804 ticks.
    let world = flat_world();
    let id = world.catalog.id_by_slug("void_goliath").expect("shipped");
    let arch = world.catalog.get(id).expect("registered");
    assert_eq!(arch.capabilities.barrier, barrier(100, 54, 804));
}
