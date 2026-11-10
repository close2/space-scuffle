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
//!
//! A burrowed body is under ground: `LurkerMPBurrowed` collides only with `Burrow` (`void`
//! UnitData:775), so ground units walk over it and it neither pushes nor is pushed by them, while
//! two burrowed bodies still keep apart (plan `2026-11-09-friendly-blocking` R6, B-93). The mode
//! says so through `UnitModeSpec::burrowed`.

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
            burrowed: false,
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
    assert!(
        mode.burrowed,
        "LurkerMPBurrowed collides only with Burrow (void UnitData:775)"
    );
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

/// A burrowing body: its one ability switches, in one tick, to a burrowed mode that is
/// `immobile` or not (a burrowed Lurker, or a unit that moves under ground) and `burrowed` or
/// not (the control: the same hidden, anchored stance above ground).
fn register_burrow_body(
    world: &mut SimWorld,
    slug: &str,
    immobile: bool,
    burrowed: bool,
) -> UnitTypeId {
    register(world, slug, |arch| {
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.modes = vec![UnitModeSpec {
            mode_name: MODE.to_string(),
            cast_ticks: 1,
            leave_cast_ticks: 1,
            cast_random_delay_max_ticks: SimScalar::ZERO,
            leave_random_delay_max_ticks: SimScalar::ZERO,
            is_immobile: immobile,
            unpushable: immobile,
            is_cloaked: true,
            burrowed,
            mover_type_override: None,
            sight_radius_override: None,
            ground_weapon_override: None,
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

/// Spawns a body of `unit_type` for `owner` at `position` and burrows it, stepping from tick
/// `*tick` until its mode is active.
fn burrow_at(
    world: &mut SimWorld,
    unit_type: UnitTypeId,
    owner: PlayerId,
    position: SimVec2,
    tick: &mut u32,
) -> EntityId {
    let body = world
        .spawn_unit(unit_type, owner, position)
        .expect("spawn body");
    assert!(
        world.execute_ability(body, 0, None, None),
        "burrow admitted"
    );
    while world.entities[&body].active_mode.is_none() {
        assert!(*tick < 20, "the one-tick burrow ends");
        world.step(&empty(*tick));
        *tick += 1;
    }
    body
}

/// The largest sideways offset from y = 10 of a walker sent from (14, 10) to (26, 10) past a body
/// burrowed at (20, 10), and whether it arrived; the body must not move.
fn walk_past(owner_of_walker: PlayerId, burrowed: bool) -> (SimScalar, bool) {
    let mut world = flat_world(0x0b93);
    let body_type = register_burrow_body(&mut world, "burrow_body_test_anchor", true, burrowed);
    let walker_type = register_target(&mut world);
    let mut tick = 0;
    let body = burrow_at(&mut world, body_type, PlayerId(0), at(20, 10), &mut tick);
    let anchor = world.entities[&body].position;
    let walker = world
        .spawn_unit(walker_type, owner_of_walker, at(14, 10))
        .expect("spawn walker");
    world
        .entities
        .get_mut(&walker)
        .expect("walker")
        .issue_order(UnitOrder::Move { target: at(26, 10) }, false)
        .expect("move order");
    let mut widest = SimScalar::ZERO;
    for _ in 0..200 {
        world.step(&empty(tick));
        tick += 1;
        let offset = (world.entities[&walker].position.y - SimScalar::from_num(10)).abs();
        widest = widest.max(offset);
    }
    assert_eq!(
        world.entities[&body].position, anchor,
        "the anchored body never moves"
    );
    let arrived = world.entities[&walker].position.x > SimScalar::from_num(25.9);
    (widest, arrived)
}

#[test]
fn b93_a_ground_unit_walks_straight_over_a_burrowed_body() {
    for owner in [PlayerId(0), PlayerId(1)] {
        let (widest, arrived) = walk_past(owner, true);
        assert!(arrived, "{owner:?}: the walker arrives");
        assert!(
            widest < SimScalar::from_num(0.01),
            "{owner:?}: the walker keeps its line over the burrowed body: offset {widest}"
        );
    }
}

#[test]
fn b93_the_same_stance_above_ground_still_blocks() {
    let (widest, _) = walk_past(PlayerId(0), false);
    assert!(
        widest > SimScalar::from_num(0.5),
        "an anchored body above ground is walked around: offset {widest}"
    );
}

#[test]
fn b93_a_burrowed_body_neither_pushes_nor_is_pushed_by_a_ground_unit() {
    let mut world = flat_world(0x0b94);
    let anchor_type = register_burrow_body(&mut world, "burrow_body_test_anchor", true, true);
    let mover_type = register_burrow_body(&mut world, "burrow_body_test_mover", false, true);
    let ground_type = register_target(&mut world);
    let mut tick = 0;
    let anchor = burrow_at(&mut world, anchor_type, PlayerId(0), at(20, 10), &mut tick);
    let mover = burrow_at(&mut world, mover_type, PlayerId(0), at(30, 10), &mut tick);
    // An idle ground unit standing half over each burrowed body, one friendly, one hostile.
    let half = SimScalar::HALF;
    let over_anchor = world
        .spawn_unit(
            ground_type,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(20) + half, SimScalar::from_num(10)),
        )
        .expect("spawn ground unit");
    let over_mover = world
        .spawn_unit(
            ground_type,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(30) + half, SimScalar::from_num(10)),
        )
        .expect("spawn ground unit");
    let before: Vec<SimVec2> = [anchor, mover, over_anchor, over_mover]
        .iter()
        .map(|id| world.entities[id].position)
        .collect();
    for _ in 0..50 {
        world.step(&empty(tick));
        tick += 1;
    }
    let after: Vec<SimVec2> = [anchor, mover, over_anchor, over_mover]
        .iter()
        .map(|id| world.entities[id].position)
        .collect();
    assert_eq!(
        after, before,
        "no body is pushed out of an overlap across the ground"
    );
}

#[test]
fn b93_burrowed_bodies_still_collide_with_each_other() {
    let mut world = flat_world(0x0b95);
    let anchor_type = register_burrow_body(&mut world, "burrow_body_test_anchor", true, true);
    let mover_type = register_burrow_body(&mut world, "burrow_body_test_mover", false, true);
    let mut tick = 0;
    let anchor = burrow_at(&mut world, anchor_type, PlayerId(0), at(20, 10), &mut tick);
    let mover = burrow_at(&mut world, mover_type, PlayerId(0), at(14, 10), &mut tick);
    world
        .entities
        .get_mut(&mover)
        .expect("mover")
        .issue_order(UnitOrder::Move { target: at(26, 10) }, false)
        .expect("move order");
    let contact = world
        .catalog
        .get(anchor_type)
        .expect("anchor")
        .collision_radius
        + world
            .catalog
            .get(mover_type)
            .expect("mover")
            .collision_radius;
    let mut widest = SimScalar::ZERO;
    for _ in 0..200 {
        world.step(&empty(tick));
        tick += 1;
        let gap = world.entities[&mover]
            .position
            .distance(world.entities[&anchor].position);
        assert!(
            gap > contact - SimScalar::from_num(0.05),
            "tick {tick}: the burrowed mover overlaps the burrowed anchor: {gap} < {contact}"
        );
        widest = widest.max((world.entities[&mover].position.y - SimScalar::from_num(10)).abs());
    }
    assert!(
        widest > SimScalar::from_num(0.5),
        "it walks round the anchor: offset {widest}"
    );
}

#[test]
fn b93_a_burrowing_cloak_toggle_burrows_and_a_mode_burrows_only_once_entered() {
    let mut world = flat_world(0x0b96);
    let toggler = register(&mut world, "burrow_body_test_toggler", |arch| {
        arch.modes = Vec::new();
        arch.abilities = vec![AbilitySpec {
            name: "Burrow".to_string(),
            target_kind: AbilityTargetKind::ToggleMode,
            energy_cost: SimScalar::ZERO,
            cooldown_ticks: 0,
            cast_range: SimScalar::ZERO,
            effect: AbilityEffectSpec::CloakToggle {
                energy_cost_per_second: SimScalar::ZERO,
                is_burrow: true,
            },
            leave_name: None,
        }];
    });
    let slow = register_burrower(&mut world, 10, 4);
    let unit = world
        .spawn_unit(toggler, PlayerId(0), at(10, 10))
        .expect("spawn toggler");
    let channelling = world
        .spawn_unit(slow, PlayerId(0), at(20, 10))
        .expect("spawn burrower");
    let burrowed = |world: &SimWorld, id: EntityId| {
        let entity = &world.entities[&id];
        entity.is_burrowed(world.catalog.get(entity.unit_type_id).expect("archetype"))
    };
    assert!(!burrowed(&world, unit) && !burrowed(&world, channelling));
    assert!(
        world.execute_ability(unit, 0, None, None),
        "toggle admitted"
    );
    assert!(
        world.execute_ability(channelling, 0, None, None),
        "burrow admitted"
    );
    world.step(&empty(0));
    assert!(
        burrowed(&world, unit),
        "the burrowing toggle puts the unit under ground"
    );
    assert!(
        !burrowed(&world, channelling),
        "a unit still channelling its burrow stands above ground"
    );
    // `register_burrower`'s mode is hidden and anchored but not marked burrowed.
    for tick in 1..20 {
        world.step(&empty(tick));
    }
    assert!(world.entities[&channelling].active_mode.is_some());
    assert!(
        !burrowed(&world, channelling),
        "only a burrowed mode burrows"
    );
    assert!(
        world.execute_ability(unit, 0, None, None),
        "toggle back admitted"
    );
    world.step(&empty(20));
    assert!(!burrowed(&world, unit), "toggled back, it is above ground");
}
