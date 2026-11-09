//! # Integration Test: a unit that attacks only in a mode (B-42)
//!
//! `StarCraft` II's Lurker carries no weapon until it burrows: the unburrowed `LurkerMP` has
//! no `WeaponArray` (`liberty` UnitData:690-758; `voidmulti` UnitData:967-985 shows the
//! disabled `Spinesdisabled` for display only), while `LurkerMPBurrowed` carries `LurkerMP`
//! (`void` UnitData:775). The burrow morph takes its longest section, `Actor` 2.5 Normal s
//! (`void` AbilData:149-155), plus up to 0.25 random (`swarm` AbilData:1663); unburrowing 0.625
//! plus up to 0.25 (`voidmulti` AbilData:181-189).
//!
//! The capability is a mode (`UnitArchetype::modes`) whose `ground_weapon_override` arms an
//! archetype that has no weapon of its own. These tests drive scratch archetypes cloned from a
//! shipped unit through `SimWorld::step`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use sim_core::catalog::AcquireLevel;
use sim_core::{SimWorld, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, AttributeBonuses, AttributeFlags, EntityId,
    PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitModeSpec, UnitOrder, UnitTypeId,
    WeaponVisualKind,
};

/// The mode name of the scratch archetype.
const MODE: &str = "burrow_attack_test_mode";

/// A 64 × 64 flat world on the shipped catalog.
fn flat_world(seed: u64) -> SimWorld {
    let cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, cell).expect("invariant: valid map");
    sim_fixtures::world(map, seed)
}

/// A hitscan weapon dealing 10 every 10 ticks at range 6, without random delay.
fn spines() -> WeaponSpec {
    WeaponSpec {
        name: "BurrowTestSpines".to_string(),
        visual_kind: WeaponVisualKind::Hitscan,
        target_flags: AttributeFlags::all(),
        min_range: SimScalar::ZERO,
        max_range: SimScalar::from_num(6),
        firing_arc: SimAngle::from_degrees(360),
        acquire_arc: SimAngle::from_degrees(360),
        has_turret: false,
        turret_turn_rate: SimAngle::ZERO,
        turret_idle: sim_types::TurretIdle::Hold,
        base_damage: SimScalar::from_num(10),
        attribute_bonuses: AttributeBonuses::NONE,
        attacks_count: 1,
        strike_interval_ticks: 0,
        followup_max_distance: None,
        channeled: false,
        period_ticks: 10,
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

/// Registers a clone of the first shipped armed mobile unit under a free id, changed by `edit`.
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
    // The first armed unit may be a worker, which acquires nothing on its own while idle.
    arch.acquire_level = AcquireLevel::Offensive;
    edit(&mut arch);
    world.catalog.register(arch);
    free
}

/// A burrower: no weapon of its own; its one ability switches to a mode that takes `cast`
/// ticks to enter and `leave` to leave, anchors and hides it, and arms it with [`spines`].
fn register_burrower(world: &mut SimWorld, cast: u16, leave: u16) -> UnitTypeId {
    register(world, "burrow_attack_test_burrower", |arch| {
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.modes = vec![UnitModeSpec {
            mode_name: MODE.to_string(),
            cast_ticks: cast,
            leave_cast_ticks: leave,
            cast_random_delay_max_ticks: SimScalar::ZERO,
            leave_random_delay_max_ticks: SimScalar::ZERO,
            is_immobile: true,
            unpushable: true,
            is_cloaked: true,
            mover_type_override: None,
            sight_radius_override: None,
            ground_weapon_override: Some(Some(spines())),
            air_weapon_override: None,
            energy_cost_per_second: SimScalar::ZERO,
        }];
        arch.abilities = vec![AbilitySpec {
            name: "Burrow".to_string(),
            target_kind: AbilityTargetKind::ToggleMode,
            energy_cost: SimScalar::ZERO,
            cooldown_ticks: 0,
            cast_range: SimScalar::ZERO,
            effect: AbilityEffectSpec::SwitchMode {
                mode_name: MODE.to_string(),
            },
            leave_name: None,
        }];
    })
}

/// A passive, unarmed target with 1000 life and no armour or regeneration.
fn register_target(world: &mut SimWorld) -> UnitTypeId {
    register(world, "burrow_attack_test_target", |arch| {
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.max_hp = SimScalar::from_num(1000);
        arch.max_shields = SimScalar::ZERO;
        arch.shield_regen_per_tick = SimScalar::ZERO;
        arch.shield_regen_delay_ticks = None;
        arch.hp_regen_per_tick = SimScalar::ZERO;
        arch.hp_regen_delay_ticks = 0;
        arch.life_armor = SimScalar::ZERO;
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

/// A burrower at (10, 10) and a holding target at (14, 10), within the spines' range.
fn duel(cast: u16, leave: u16) -> (SimWorld, EntityId, EntityId) {
    let mut world = flat_world(0x0b42);
    let burrower = register_burrower(&mut world, cast, leave);
    let target = register_target(&mut world);
    let shooter = world
        .spawn_unit(burrower, PlayerId(0), at(10, 10))
        .expect("spawn burrower");
    let victim = world
        .spawn_unit(target, PlayerId(1), at(14, 10))
        .expect("spawn target");
    world
        .entities
        .get_mut(&victim)
        .expect("victim")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold");
    (world, shooter, victim)
}

/// Steps `world` from tick `from` for `ticks` ticks and returns the ticks on which `victim` lost
/// life.
fn losses(world: &mut SimWorld, victim: EntityId, from: u32, ticks: u32) -> Vec<u32> {
    let mut hits = Vec::new();
    for tick in from..from + ticks {
        let before = world.entities[&victim].hp;
        world.step(&empty(tick));
        if world.entities[&victim].hp < before {
            hits.push(tick);
        }
    }
    hits
}

#[test]
fn a_unit_without_a_weapon_of_its_own_never_attacks_unburrowed() {
    let (mut world, _shooter, victim) = duel(10, 4);
    let hits = losses(&mut world, victim, 0, 100);
    assert!(hits.is_empty(), "no weapon outside the mode: {hits:?}");
}

#[test]
fn an_attack_order_on_an_unburrowed_unit_without_a_weapon_deals_nothing() {
    let (mut world, shooter, victim) = duel(10, 4);
    world
        .entities
        .get_mut(&shooter)
        .expect("shooter")
        .issue_order(UnitOrder::attack(victim), false)
        .expect("attack order");
    let hits = losses(&mut world, victim, 0, 100);
    assert!(hits.is_empty(), "an ordered attack has no weapon: {hits:?}");
}

#[test]
fn burrowing_arms_the_unit_only_after_its_cast_time() {
    let (mut world, shooter, victim) = duel(10, 4);
    assert!(
        world.execute_ability(shooter, 0, None, None),
        "burrow admitted"
    );
    let hits = losses(&mut world, victim, 0, 60);
    let first = *hits.first().expect("the burrowed unit attacks");
    assert!(
        first >= 10,
        "no attack before the morph ends: first hit on tick {first}"
    );
    assert!(
        first <= 13,
        "it attacks as soon as it is burrowed: first hit on tick {first}"
    );
    let unit = &world.entities[&shooter];
    assert!(unit.is_immobile && unit.unpushable && unit.is_cloaked);
    assert_eq!(unit.active_mode.as_deref(), Some(MODE));
}

#[test]
fn unburrowing_disarms_the_unit_and_frees_it_to_move() {
    let (mut world, shooter, victim) = duel(10, 4);
    assert!(
        world.execute_ability(shooter, 0, None, None),
        "burrow admitted"
    );
    let armed = losses(&mut world, victim, 0, 40);
    assert!(!armed.is_empty(), "burrowed, it attacks");
    assert!(
        world.execute_ability(shooter, 0, None, None),
        "unburrow admitted"
    );
    let after = losses(&mut world, victim, 40, 60);
    // A hit already released when the unburrow began may still land on its first tick.
    assert!(
        after.iter().all(|&tick| tick <= 41),
        "unburrowed, it no longer attacks: {after:?}"
    );
    let unit = &world.entities[&shooter];
    assert!(!unit.is_immobile && !unit.is_cloaked && unit.active_mode.is_none());
    let start = unit.position;
    world
        .entities
        .get_mut(&shooter)
        .expect("shooter")
        .issue_order(UnitOrder::Move { target: at(10, 30) }, false)
        .expect("move order");
    losses(&mut world, victim, 100, 40);
    assert!(
        world.entities[&shooter].position.y > start.y + SimScalar::from_num(2),
        "it walks again"
    );
}

#[test]
fn an_unburrowed_unit_without_a_weapon_attack_moves_past_an_enemy() {
    let (mut world, shooter, victim) = duel(10, 4);
    world
        .entities
        .get_mut(&shooter)
        .expect("shooter")
        .issue_order(UnitOrder::AttackMove { target: at(30, 10) }, false)
        .expect("attack-move");
    let hits = losses(&mut world, victim, 0, 300);
    assert!(hits.is_empty(), "nothing to attack with: {hits:?}");
    assert!(
        world.entities[&shooter].position.x > SimScalar::from_num(25),
        "it is not held by the enemy it cannot attack: x = {}",
        world.entities[&shooter].position.x
    );
}

/// `burrow_scorpion`, the Lurker's analogue, attacks only burrowed, with `StarCraft` II's times.
#[test]
fn b42_the_lurker_analogue_attacks_only_burrowed() {
    let world = flat_world(1);
    let lurker = world
        .catalog
        .id_by_slug("burrow_scorpion")
        .expect("burrow_scorpion");
    let arch = world.catalog.get(lurker).expect("archetype");
    assert!(
        arch.weapon.is_none() && arch.ground_weapon.is_none() && arch.air_weapon.is_none(),
        "unburrowed it has no weapon (liberty UnitData:690-758)"
    );
    let mode = arch.get_mode("burrowed").expect("a burrowed mode");
    // The weapon and the cloak arrive at the end of the morph's Stats section, 1.8332 Normal s
    // (owner answer to Q0069) x 25 / 1.4 = 32.74; 0.625 -> 11.16; 0.25 -> 4.4643. The Actor
    // section (2.5 s, 45 ticks) is the burrow animation only.
    assert_eq!(
        mode.cast_ticks, 33,
        "swarm AbilData:1654-1674, void :149-155: Stats delay 1.8332"
    );
    assert_eq!(
        mode.leave_cast_ticks, 11,
        "voidmulti AbilData:187 Actor 0.625"
    );
    assert_eq!(
        mode.cast_random_delay_max_ticks,
        SimScalar::from_f32_lossy(4.4643)
    );
    assert_eq!(
        mode.leave_random_delay_max_ticks,
        SimScalar::from_f32_lossy(4.4643)
    );
    assert!(mode.is_immobile && mode.unpushable && mode.is_cloaked);
    let spines = mode
        .ground_weapon_override
        .clone()
        .flatten()
        .expect("burrowed it carries its spines (void UnitData:775)");
    assert_eq!(
        spines.max_range,
        SimScalar::from_num(8),
        "voidmulti WeaponData:145"
    );
    assert_eq!(spines.period_ticks, 36);
    assert_eq!(spines.base_damage, SimScalar::from_num(20));
    assert!(arch.abilities.iter().any(|ability| matches!(
        &ability.effect,
        AbilityEffectSpec::SwitchMode { mode_name } if mode_name == "burrowed"
    )));
}
