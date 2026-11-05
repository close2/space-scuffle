//! Combat correctness: each remediation finding replayed through the public tick API.
//!
//! # Responsibility
//! Pins the player-visible combat rules that the Phase 3 audit found broken: strike counts,
//! splash layers, order fidelity, hold position, target loss, minimum range and availability.
//!
//! # How it fits
//! Damage formulas have unit-level coverage in `damage_and_shields_tests.rs`. This suite
//! drives `SimWorld::step` and asserts what a player would see, so it fails when a rule is
//! right in isolation but unreachable or undone in the tick pipeline.
//!
//! # Invariants
//! - Orders go through `PlayerCommand`. Direct state edits are confined to rule fixtures a
//!   command cannot express, such as authoring a catalog weapon, and are commented.
//! - Each test names the finding it covers, and asserts the correct behaviour.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]

use sim_core::{SimCoreError, SimWorld, UpgradeCatalog, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, AttackOrigin, EntityId, PlayerCommand,
    PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, Tick, TickBundle, UnitModeSpec, UnitOrder,
    VisibilityState,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// A flat arena stepped only through tick bundles.
struct Arena {
    world: SimWorld,
    tick: u32,
}

impl Arena {
    fn new(cells: u32) -> Self {
        let map = GameMap::new(cells, cells, MapCell::default_ground(CliffLevel(0)))
            .expect("invariant: test maps have a valid size");
        Self {
            world: sim_fixtures::world(map, 7),
            tick: 0,
        }
    }

    fn spawn(&mut self, slug: &str, owner: PlayerId, x: f64, y: f64) -> EntityId {
        let unit_type = self
            .world
            .catalog
            .id_by_slug(slug)
            .unwrap_or_else(|| panic!("catalog lacks {slug}"));
        self.world
            .spawn_unit(unit_type, owner, point(x, y))
            .expect("spawn fits the arena")
    }

    fn step(&mut self, commands: Vec<PlayerCommand>) {
        self.world.step(&TickBundle {
            tick: Tick(self.tick),
            commands,
            state_checksum: None,
        });
        self.tick += 1;
    }

    fn order(&mut self, player: PlayerId, entities: &[EntityId], order: UnitOrder) {
        self.step(vec![PlayerCommand {
            player_id: player,
            entities: entities.to_vec(),
            order,
            queue: false,
        }]);
    }

    fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.step(Vec::new());
        }
    }

    /// Hull plus shields, or zero once the entity is dead or reaped.
    fn vitals(&self, id: EntityId) -> f32 {
        self.world
            .entities
            .get(&id)
            .map_or(0.0, |e| e.hp.to_f32() + e.shields.to_f32())
    }
}

fn point(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn attack(target: EntityId) -> UnitOrder {
    UnitOrder::attack(target)
}

/// Whether `unit` is currently under an attack order on `target`.
fn front_attacks(arena: &Arena, unit: EntityId, target: EntityId) -> bool {
    matches!(
        arena.world.entities[&unit].order_queue.front(),
        Some(UnitOrder::AttackTarget { target: current, .. }) if *current == target
    )
}

/// Runs until `unit`'s weapon enters its wind-up, failing the test if it never does.
fn run_to_windup(arena: &mut Arena, unit: EntityId) {
    for _ in 0..60 {
        if matches!(
            arena.world.entities[&unit].weapon_phase,
            WeaponPhase::Windup { .. }
        ) {
            return;
        }
        arena.run(1);
    }
    panic!("weapon never reached its wind-up");
}

/// Sieges `tank` through its mode ability and waits out the transformation.
fn siege(arena: &mut Arena, tank: EntityId) {
    arena.order(
        BLUE,
        &[tank],
        UnitOrder::CastAbility {
            ability_index: 0,
            target_pos: None,
            target_entity: None,
        },
    );
    arena.run(80);
    assert!(arena.world.entities[&tank].is_sieged);
}

/// S-4: a projectile weapon's `attacks_count` strikes all land, as on the instant path.
#[test]
fn s4_projectile_weapon_applies_every_strike() {
    let mut arena = Arena::new(32);
    // The first shipped archetype whose ground weapon fires several projectile strikes with no
    // bonus damage, so each strike deals its base damage minus the victim's armour.
    let shooter_slug = arena
        .world
        .catalog
        .archetypes()
        .into_iter()
        .find(|a| {
            a.weapon.as_ref().is_some_and(|w| {
                w.attacks_count >= 2
                    && w.projectile_speed.is_some()
                    && w.splash.is_none()
                    && w.bonus_armored == SimScalar::ZERO
                    && w.bonus_light == SimScalar::ZERO
                    && w.max_range >= SimScalar::from_num(3)
            })
        })
        .expect("the catalog has a multi-strike projectile weapon")
        .slug
        .clone();
    let shooter = arena.spawn(&shooter_slug, BLUE, 10.0, 10.0);
    let victim = arena.spawn("shock_golem", RED, 13.0, 10.0);
    let unit_type = arena.world.entities[&shooter].unit_type_id;
    let weapon = arena
        .world
        .catalog
        .get(unit_type)
        .unwrap()
        .weapon
        .clone()
        .unwrap();
    assert_eq!(weapon.attacks_count, 2);
    assert!(weapon.projectile_speed.is_some());
    let target_arch = arena
        .world
        .catalog
        .get(arena.world.entities[&victim].unit_type_id)
        .unwrap();
    let per_strike = (weapon.base_damage - target_arch.life_armor).to_f32();

    let before = arena.vitals(victim);
    arena.order(BLUE, &[shooter], attack(victim));
    let shots = weapon.period_ticks;
    let mut first_damage = None;
    for _ in 0..(u32::from(shots) * 3) {
        arena.run(1);
        if arena.vitals(victim) < before {
            first_damage = Some(before - arena.vitals(victim));
            break;
        }
    }
    assert_eq!(first_damage, Some(per_strike * 2.0));
}

/// S-13: splash from a shot at a ground target leaves aircraft overhead untouched.
#[test]
fn s13_ground_splash_does_not_reach_air() {
    let mut arena = Arena::new(32);
    let tank = arena.spawn("railgun_crawler", BLUE, 10.0, 10.0);
    siege(&mut arena, tank);

    let ground_target = arena.spawn("rhino_beetle", RED, 18.0, 10.0);
    let ground_neighbour = arena.spawn("rhino_beetle", RED, 18.0, 10.4);
    let air_neighbour = arena.spawn("sonic_bat", RED, 18.0, 10.2);
    let (near_hp, air_hp) = (arena.vitals(ground_neighbour), arena.vitals(air_neighbour));

    arena.order(BLUE, &[tank], attack(ground_target));
    arena.run(40);
    assert!(
        arena.vitals(ground_neighbour) < near_hp,
        "the ground neighbour proves the shell landed"
    );
    assert!(
        (arena.vitals(air_neighbour) - air_hp).abs() < f32::EPSILON,
        "ground splash damaged an aircraft"
    );
}

/// S-13: a unit never damages itself with its own area weapon, on any delivery path.
#[test]
fn s13_projectile_splash_spares_its_attacker() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("jet_jumper", BLUE, 10.0, 10.0);
    // An unarmed structure keeps retaliation out of the attacker's vitals.
    let dome = arena.world.catalog.id_by_slug("foundry_core_dome").unwrap();
    let victim = arena.world.spawn_structure(dome, RED, 13, 8).unwrap();
    // Rule fixture: authoring a friendly-fire projectile splash no shipped ground unit has.
    let unit_type = arena.world.entities[&shooter].unit_type_id;
    let weapon = arena
        .world
        .catalog
        .get_mut(unit_type)
        .unwrap()
        .weapon
        .as_mut()
        .unwrap();
    weapon.splash = Some(sim_core::SplashSpec {
        inner_radius: SimScalar::from_num(10),
        middle_radius: SimScalar::from_num(11),
        outer_radius: SimScalar::from_num(12),
        friendly_fire: true,
    });
    let hp = arena.vitals(shooter);
    let victim_hp = arena.vitals(victim);
    arena.order(BLUE, &[shooter], attack(victim));
    arena.run(30);
    assert!(arena.vitals(victim) < victim_hp, "the shot must land");
    assert!(
        (arena.vitals(shooter) - hp).abs() < f32::EPSILON,
        "attacker damaged itself"
    );
}

/// S-1, reproduction r01: a holding unit shoots an enemy inside its weapon range.
#[test]
fn s1_hold_position_fires_at_enemy_in_range() {
    let mut arena = Arena::new(32);
    let holder = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let enemy = arena.spawn("rhino_beetle", RED, 12.0, 10.0);
    let start = arena.world.entities[&holder].position;
    let hp = arena.vitals(enemy);
    arena.order(BLUE, &[holder], UnitOrder::HoldPosition);
    arena.run(200);
    assert!(arena.vitals(enemy) < hp, "holder never damaged the enemy");
    if let Some(unit) = arena.world.entities.get(&holder) {
        assert!(
            unit.position.distance(start) < SimScalar::from_num(1),
            "holding must not chase"
        );
    }
}

/// S-1, reproduction r02: the idle control fires, so r01 is not a setup fault.
#[test]
fn s1_control_idle_unit_fires() {
    let mut arena = Arena::new(32);
    arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let enemy = arena.spawn("rhino_beetle", RED, 12.0, 10.0);
    let hp = arena.vitals(enemy);
    arena.run(200);
    assert!(arena.vitals(enemy) < hp);
}

/// S-1: holding means no pursuit, however visible the enemy.
#[test]
fn s1_hold_position_does_not_pursue_beyond_range() {
    let mut arena = Arena::new(32);
    let holder = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let enemy = arena.spawn("pulse_bot", RED, 17.5, 10.0);
    arena.order(RED, &[enemy], UnitOrder::HoldPosition);
    arena.order(BLUE, &[holder], UnitOrder::HoldPosition);
    let start = arena.world.entities[&holder].position;
    arena.run(100);
    assert_eq!(arena.world.entities[&holder].position, start);
    assert!(
        matches!(
            arena.world.entities[&holder].order_queue.front(),
            Some(UnitOrder::HoldPosition)
        ),
        "the hold order must survive acquisition"
    );
    let unit_type = arena.world.entities[&enemy].unit_type_id;
    let full = arena.world.catalog.get(unit_type).unwrap().max_hp;
    assert_eq!(
        arena.world.entities[&enemy].hp, full,
        "enemy is out of range"
    );
}

/// S-2, reproduction r05: an explicit focus-fire order survives a nearer enemy.
#[test]
fn s2_explicit_focus_fire_survives_nearby_enemy() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    arena.spawn("psi_grey", RED, 12.0, 10.0);
    let far = arena.spawn("rhino_beetle", RED, 18.0, 10.0);
    // The far target holds its ground, so how soon the shooter dies does not depend on how
    // fast the target walks into its own range; only the nearby enemy fights the shooter.
    arena.order(RED, &[far], UnitOrder::HoldPosition);
    arena.run(2);
    arena.order(BLUE, &[shooter], attack(far));
    arena.run(30);
    assert!(
        front_attacks(&arena, shooter, far),
        "explicit order replaced"
    );
}

/// S-2: an explicit order stays explicit when more orders are queued behind it.
#[test]
fn s2_explicit_focus_fire_survives_with_queued_orders() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    arena.spawn("psi_grey", RED, 12.0, 10.0);
    let far = arena.spawn("rhino_beetle", RED, 18.0, 10.0);
    arena.run(2);
    arena.step(vec![
        PlayerCommand {
            player_id: BLUE,
            entities: vec![shooter],
            order: attack(far),
            queue: false,
        },
        PlayerCommand {
            player_id: BLUE,
            entities: vec![shooter],
            order: UnitOrder::Move {
                target: point(5.0, 5.0),
            },
            queue: true,
        },
    ]);
    arena.run(10);
    assert!(front_attacks(&arena, shooter, far));
}

/// S-2: acquired targets do not flap to an equally rated enemy that merely got closer.
#[test]
fn s2_acquired_target_has_hysteresis() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let first = arena.spawn("rhino_beetle", RED, 14.0, 10.0);
    arena.order(
        BLUE,
        &[shooter],
        UnitOrder::AttackMove {
            target: point(20.0, 10.0),
        },
    );
    arena.order(RED, &[first], UnitOrder::HoldPosition);
    arena.run(3);
    assert!(front_attacks(&arena, shooter, first));

    let closer = arena.spawn("rhino_beetle", RED, 12.0, 10.0);
    arena.order(RED, &[closer], UnitOrder::HoldPosition);
    arena.run(5);
    assert!(
        front_attacks(&arena, shooter, first),
        "an in-range target must not be dropped for an equal, closer one"
    );
}

/// S-2: hysteresis still yields to a strictly more dangerous target.
#[test]
fn s2_acquired_target_yields_to_higher_threat() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let dome = arena.world.catalog.id_by_slug("foundry_core_dome").unwrap();
    let passive = arena.world.spawn_structure(dome, RED, 13, 8).unwrap();
    arena.run(3);
    assert!(front_attacks(&arena, shooter, passive));
    let armed = arena.spawn("rhino_beetle", RED, 10.0, 13.0);
    arena.run(3);
    assert!(front_attacks(&arena, shooter, armed));
}

/// S-5: friendly-fire splash neither turns allies on their own unit nor scares them away.
#[test]
fn s5_friendly_splash_does_not_provoke_allies() {
    let mut arena = Arena::new(32);
    let tank = arena.spawn("railgun_crawler", BLUE, 10.0, 10.0);
    siege(&mut arena, tank);
    let enemy = arena.spawn("rhino_beetle", RED, 18.0, 10.0);
    let ally = arena.spawn("shock_golem", BLUE, 18.0, 10.6);
    // Rule fixture: with no scan range neither unit engages the other, so only the reaction
    // to the friendly shell can change the ally's orders.
    for unit in [enemy, ally] {
        let unit_type = arena.world.entities[&unit].unit_type_id;
        arena.world.catalog.get_mut(unit_type).unwrap().scan_range = SimScalar::ZERO;
    }
    arena.order(RED, &[enemy], UnitOrder::HoldPosition);
    let hp = arena.vitals(ally);
    let position = arena.world.entities[&ally].position;
    arena.order(BLUE, &[tank], attack(enemy));
    let mut splashed = false;
    for _ in 0..80 {
        arena.run(1);
        splashed |= arena.vitals(ally) < hp;
        assert!(
            !front_attacks(&arena, ally, tank),
            "ally turned on its own tank"
        );
    }
    assert!(splashed, "the fixture must splash the ally");
    assert_eq!(
        arena.world.entities[&ally].position, position,
        "ally must not flee from friendly splash"
    );
}

/// S-6: a shot goes to the target its swing started on, not whatever is current at release.
#[test]
fn s6_windup_fires_at_its_bound_target() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let bound = arena.spawn("rhino_beetle", RED, 13.0, 10.0);
    let other = arena.spawn("rhino_beetle", RED, 10.0, 13.0);
    let hp = (arena.vitals(bound), arena.vitals(other));
    arena.order(BLUE, &[shooter], attack(bound));
    run_to_windup(&mut arena, shooter);
    let WeaponPhase::Windup { target_id, .. } = arena.world.entities[&shooter].weapon_phase else {
        panic!("the shooter must reach its wind-up");
    };
    assert_eq!(target_id, bound);
    // Rule fixture: the order changes mid-swing without the interruption a command applies,
    // which is what an automatic retarget during wind-up does.
    let unit = arena.world.entities.get_mut(&shooter).unwrap();
    if let Some(order) = unit.order_queue.front_mut() {
        *order = attack(other);
    }
    arena.run(3);
    assert!(
        arena.vitals(bound) < hp.0,
        "the bound target takes the shot"
    );
    assert!((arena.vitals(other) - hp.1).abs() < f32::EPSILON);
}

/// S-6, S-12: a target that left range during the swing is neither hit nor paid for.
#[test]
fn s6_windup_rechecks_range_at_release() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let target = arena.spawn("rhino_beetle", RED, 13.0, 10.0);
    let hp = arena.vitals(target);
    arena.order(BLUE, &[shooter], attack(target));
    run_to_windup(&mut arena, shooter);
    // Rule fixture: a displacement no tick of movement can produce within one swing.
    arena.world.entities.get_mut(&target).unwrap().position = point(26.0, 26.0);
    arena.run(5);
    assert!(
        (arena.vitals(target) - hp).abs() < f32::EPSILON,
        "shot landed from beyond range"
    );
    assert_eq!(
        arena.world.entities[&shooter].weapon_cooldown_remaining, 0,
        "a cancelled swing costs no cooldown"
    );
}

/// S-7: a sieged unit drops a target inside its minimum range and engages one outside it.
#[test]
fn s7_sieged_unit_retargets_inside_minimum_range() {
    let mut arena = Arena::new(32);
    let tank = arena.spawn("railgun_crawler", BLUE, 10.0, 10.0);
    siege(&mut arena, tank);
    let close = arena.spawn("rhino_beetle", RED, 18.0, 10.0);
    arena.order(RED, &[close], UnitOrder::HoldPosition);
    arena.run(1);
    assert!(front_attacks(&arena, tank, close));
    // Rule fixture: the walk under the gun, condensed so shells cannot end the test first.
    arena.world.entities.get_mut(&close).unwrap().position = point(11.2, 10.0);
    let far = arena.spawn("rhino_beetle", RED, 20.0, 10.0);
    arena.order(RED, &[far], UnitOrder::HoldPosition);
    let hp = arena.vitals(far);
    arena.run(150);
    assert!(arena.vitals(far) < hp, "tank locked onto its blind spot");
}

/// S-7: an explicit target inside minimum range releases an immobile unit too.
#[test]
fn s7_sieged_unit_abandons_explicit_target_inside_minimum_range() {
    let mut arena = Arena::new(32);
    let tank = arena.spawn("railgun_crawler", BLUE, 10.0, 10.0);
    siege(&mut arena, tank);
    let close = arena.spawn("rhino_beetle", RED, 11.5, 10.0);
    arena.order(RED, &[close], UnitOrder::HoldPosition);
    arena.order(BLUE, &[tank], attack(close));
    arena.run(3);
    assert!(arena.world.entities[&tank].order_queue.is_empty());
}

/// S-12: a target dying during the swing costs no cooldown.
#[test]
fn s12_target_death_in_windup_burns_no_cooldown() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("railgun_crawler", BLUE, 10.0, 10.0);
    let target = arena.spawn("rhino_beetle", RED, 14.0, 10.0);
    arena.order(BLUE, &[shooter], attack(target));
    run_to_windup(&mut arena, shooter);
    // Rule fixture: another source kills the target; nothing in this duel could.
    arena.world.entities.get_mut(&target).unwrap().hp = SimScalar::ZERO;
    arena.run(10);
    assert_eq!(arena.world.entities[&shooter].weapon_cooldown_remaining, 0);
}

/// S-17: no shot or acquisition reaches an invulnerable or undetected cloaked unit.
#[test]
fn s17_attack_path_respects_availability() {
    for (invulnerable, cloaked) in [(true, false), (false, true)] {
        let mut arena = Arena::new(32);
        let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
        let target = arena.spawn("rhino_beetle", RED, 12.0, 10.0);
        arena.order(RED, &[target], UnitOrder::Stop);
        let hidden = arena.world.entities.get_mut(&target).unwrap();
        // Rule fixture: no shipped ability grants these states yet.
        hidden.is_invulnerable = invulnerable;
        hidden.is_cloaked = cloaked;
        let hp = arena.vitals(target);
        arena.order(BLUE, &[shooter], UnitOrder::Stop);
        arena.run(60);
        assert!((arena.vitals(target) - hp).abs() < f32::EPSILON);
    }
}

/// S-17: losing availability during the swing cancels the shot.
#[test]
fn s17_availability_is_rechecked_at_release() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("railgun_crawler", BLUE, 10.0, 10.0);
    let target = arena.spawn("rhino_beetle", RED, 14.0, 10.0);
    let hp = arena.vitals(target);
    arena.order(BLUE, &[shooter], attack(target));
    run_to_windup(&mut arena, shooter);
    // Rule fixture: no shipped ability grants invulnerability yet.
    let unit = arena.world.entities.get_mut(&target).unwrap();
    unit.is_invulnerable = true;
    arena.run(40);
    assert!((arena.vitals(target) - hp).abs() < f32::EPSILON);
}

/// S-17: splash stops at invulnerability but, as in SC2, reaches undetected cloaked units.
#[test]
fn s17_splash_respects_invulnerability_only() {
    let mut arena = Arena::new(32);
    let tank = arena.spawn("railgun_crawler", BLUE, 10.0, 10.0);
    siege(&mut arena, tank);
    let target = arena.spawn("rhino_beetle", RED, 18.0, 10.0);
    let shielded = arena.spawn("rhino_beetle", RED, 18.0, 10.3);
    let cloaked = arena.spawn("rhino_beetle", RED, 18.3, 10.0);
    for unit in [target, shielded, cloaked] {
        arena.order(RED, &[unit], UnitOrder::HoldPosition);
    }
    // Rule fixture: no shipped ability grants these states yet.
    let unit = arena.world.entities.get_mut(&shielded).unwrap();
    unit.is_invulnerable = true;
    let unit = arena.world.entities.get_mut(&cloaked).unwrap();
    unit.is_cloaked = true;
    let (shielded_hp, cloaked_hp) = (arena.vitals(shielded), arena.vitals(cloaked));
    arena.order(BLUE, &[tank], attack(target));
    arena.run(30);
    assert!((arena.vitals(shielded) - shielded_hp).abs() < f32::EPSILON);
    assert!(arena.vitals(cloaked) < cloaked_hp);
}

/// Authors a stance pair on `pulse_bot`: an armed siege stance and a plain second stance.
fn author_stances(arena: &mut Arena, cast_ticks: u16) -> EntityId {
    let unit = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let unit_type = arena.world.entities[&unit].unit_type_id;
    let arch = arena.world.catalog.get_mut(unit_type).unwrap();
    let weapon = arch.weapon.clone();
    let stance = |mode_name: &str, ground_weapon_override| UnitModeSpec {
        mode_name: mode_name.to_owned(),
        cast_ticks,
        is_immobile: true,
        unpushable: true,
        is_cloaked: false,
        mover_type_override: None,
        sight_radius_override: None,
        ground_weapon_override,
        air_weapon_override: None,
        energy_cost_per_second: SimScalar::ZERO,
    };
    let switch = |name: &str| AbilitySpec {
        name: name.to_owned(),
        target_kind: AbilityTargetKind::ToggleMode,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 0,
        cast_range: SimScalar::ZERO,
        effect: AbilityEffectSpec::SwitchMode {
            mode_name: name.to_owned(),
        },
    };
    arch.modes = vec![stance("armed", Some(weapon)), stance("plain", None)];
    arch.abilities = vec![switch("armed"), switch("plain")];
    unit
}

fn cast(arena: &mut Arena, unit: EntityId, ability_index: u8) {
    arena.order(
        BLUE,
        &[unit],
        UnitOrder::CastAbility {
            ability_index,
            target_pos: None,
            target_entity: None,
        },
    );
    arena.run(6);
}

/// S-18: instant and channelled mode changes reach the same stance.
#[test]
fn s18_instant_and_channelled_transitions_agree() {
    let snapshot = |cast_ticks| {
        let mut arena = Arena::new(32);
        let unit = author_stances(&mut arena, cast_ticks);
        cast(&mut arena, unit, 0);
        cast(&mut arena, unit, 1);
        let entity = &arena.world.entities[&unit];
        (
            entity.active_mode.clone(),
            entity.is_sieged,
            entity.siege_weapon.is_some(),
            entity.is_immobile,
            entity.unpushable,
        )
    };
    assert_eq!(snapshot(0), snapshot(3));
    assert_eq!(
        snapshot(3),
        (Some("plain".to_owned()), false, false, true, true)
    );
}

/// S-19: a shielded structure regenerates like the units of its faction.
#[test]
fn s19_shielded_structures_regenerate() {
    let mut arena = Arena::new(32);
    let unit_type = arena
        .world
        .catalog
        .archetypes()
        .find(|a| {
            matches!(a.shape, ShapeKind::BuildingDome { .. }) && a.max_shields > SimScalar::ZERO
        })
        .map(|a| a.unit_type_id)
        .unwrap();
    let dome = arena.world.spawn_structure(unit_type, BLUE, 8, 8).unwrap();
    // Rule fixture: damage with no attacker in the arena to complicate the regeneration delay.
    let hurt = SimScalar::from_num(10);
    let entity = arena.world.entities.get_mut(&dome).unwrap();
    entity.shields = hurt;
    arena.run(300);
    assert!(arena.world.entities[&dome].shields > hurt);
}

/// S-10: researching twice is refused and leaves the first result standing.
#[test]
fn s10_repeated_research_is_rejected() {
    let mut arena = Arena::new(32);
    let upgrade = "chitinous_plating";
    arena.world.research_upgrade(BLUE, upgrade).unwrap();
    let after_first = arena
        .world
        .player_upgrade_state(BLUE)
        .expect("invariant: BLUE has a player slot")
        .clone();
    assert!(matches!(
        arena.world.research_upgrade(BLUE, upgrade),
        Err(SimCoreError::UpgradeAlreadyResearched(_))
    ));
    assert_eq!(
        *arena
            .world
            .player_upgrade_state(BLUE)
            .expect("invariant: BLUE has a player slot"),
        after_first
    );
}

/// S-10: a repeat also must not grow live vitals a second time.
#[test]
fn s10_repeated_research_does_not_stack_live_vitals() {
    let mut arena = Arena::new(32);
    let unit = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    arena.world.research_upgrade(BLUE, "combat_shield").unwrap();
    let hp = arena.world.entities[&unit].max_hp;
    let repeat = arena.world.research_upgrade(BLUE, "combat_shield");
    assert!(repeat.is_err());
    assert_eq!(arena.world.entities[&unit].max_hp, hp);
}

/// S-11: a sight upgrade widens what the unit reveals.
#[test]
fn s11_sight_delta_extends_vision() {
    let mut arena = Arena::new(64);
    let unit = arena.spawn("pulse_bot", BLUE, 20.0, 20.0);
    let unit_type = arena.world.entities[&unit].unit_type_id;
    let arch = arena.world.catalog.get(unit_type).unwrap();
    let slug = arch.slug.clone();
    let probe = 20 + arch.sight_radius.to_f32() as u32 + 2;
    arena.run(1);
    assert_ne!(
        arena.world.vision_maps[PlayerSlot::ALL[0]].visibility_at(probe, 20),
        VisibilityState::Visible
    );
    // Rule fixture: no shipped upgrade grants sight yet.
    arena
        .world
        .player_upgrade_state_mut(BLUE)
        .expect("invariant: BLUE has a player slot")
        .unit_stat_modifiers
        .entry(slug)
        .or_default()
        .sight_delta = SimScalar::from_num(4);
    arena.run(1);
    assert_eq!(
        arena.world.vision_maps[PlayerSlot::ALL[0]].visibility_at(probe, 20),
        VisibilityState::Visible
    );
}

/// S-11: an unlock gates its capability until the owner completes the research.
#[test]
fn s11_unlock_gates_overclock_until_researched() {
    let mut arena = Arena::new(32);
    let unit = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    arena.order(BLUE, &[unit], UnitOrder::UseStim);
    assert!(
        arena.world.entities[&unit].active_buffs.is_empty(),
        "overclock must stay locked before its research completes"
    );
    arena
        .world
        .research_upgrade(BLUE, "overclock_stim")
        .unwrap();
    arena.order(BLUE, &[unit], UnitOrder::UseStim);
    assert_eq!(arena.world.entities[&unit].active_buffs.len(), 1);
}

/// Q0045 3b (combat alignment WP15): the Stimpack research unlocks the Overclock of the
/// Marauder analogue, which pays its catalog life cost and carries the buff for the catalog
/// duration; the Roach analogue has no Stimpack and does not stim.
#[test]
fn overclock_research_stims_the_marauder_analogue_and_not_the_roach_analogue() {
    let mut arena = Arena::new(32);
    let golem = arena.spawn("shock_golem", BLUE, 10.0, 10.0);
    let beetle = arena.spawn("rhino_beetle", BLUE, 14.0, 10.0);
    let golem_type = arena.world.entities[&golem].unit_type_id;
    let spec = arena
        .world
        .catalog
        .get(golem_type)
        .unwrap()
        .overclock
        .expect("the Marauder analogue carries the Overclock");
    arena
        .world
        .research_upgrade(BLUE, "overclock_stim")
        .unwrap();
    let golem_life = arena.world.entities[&golem].hp;
    let beetle_life = arena.world.entities[&beetle].hp;
    arena.order(BLUE, &[golem, beetle], UnitOrder::UseStim);

    let stimmed = &arena.world.entities[&golem];
    assert_eq!(golem_life - stimmed.hp, spec.hp_cost);
    let [buff] = stimmed.active_buffs.as_slice() else {
        panic!("one Overclock buff: {:?}", stimmed.active_buffs);
    };
    // The buff is applied during the ordering tick and counts that tick down once.
    assert_eq!(
        u32::from(buff.remaining_ticks) + 1,
        u32::from(spec.duration_ticks)
    );
    assert_eq!(buff.speed_multiplier, spec.speed_multiplier);

    let unstimmed = &arena.world.entities[&beetle];
    assert!(
        unstimmed.active_buffs.is_empty(),
        "the Roach analogue has no Stimpack"
    );
    assert_eq!(unstimmed.hp, beetle_life);
}

/// S-11: an ability named by an unlock stays locked until researched too.
#[test]
fn s11_unlock_gates_abilities_by_name() {
    const UPGRADES: &str = r#"(upgrades: [(
        upgrade_id: 1, slug: "learn", name: "learn", display_name: "Learn",
        faction: "automaton_directorate", level: 1, mineral_cost: 0, gas_cost: 0,
        research_ticks: 1, researched_at: "foundry_core_dome",
        effects: [UnlockAbility(target_slug: "pulse_bot", ability_slug: "armed")],
    )])"#;
    let mut arena = Arena::new(32);
    let unit = author_stances(&mut arena, 0);
    arena.world.upgrade_catalog =
        UpgradeCatalog::from_ron_str(&sim_fixtures::shipped_factions(), UPGRADES).unwrap();
    cast(&mut arena, unit, 0);
    assert_eq!(arena.world.entities[&unit].active_mode, None);
    arena.world.research_upgrade(BLUE, "learn").unwrap();
    cast(&mut arena, unit, 0);
    assert_eq!(
        arena.world.entities[&unit].active_mode.as_deref(),
        Some("armed")
    );
}

/// S-2: a command cannot pass itself off as automatic and so become replaceable.
#[test]
fn s2_commanded_attacks_are_always_explicit() {
    let mut arena = Arena::new(32);
    let shooter = arena.spawn("pulse_bot", BLUE, 10.0, 10.0);
    let target = arena.spawn("rhino_beetle", RED, 18.0, 10.0);
    arena.order(
        BLUE,
        &[shooter],
        UnitOrder::AttackTarget {
            target,
            origin: AttackOrigin::Acquired,
        },
    );
    assert_eq!(
        arena.world.entities[&shooter].order_queue.front(),
        Some(&UnitOrder::attack(target))
    );
}

/// S-13: ground and air are separate planes, and a cliff walker stands in both.
#[test]
fn s13_combat_planes() {
    use sim_core::combat::shares_combat_plane;
    use sim_types::MoverType::{Air, CliffJumper, CliffWalker, Ground};
    assert!(shares_combat_plane(Ground, CliffJumper));
    assert!(shares_combat_plane(Air, Air));
    assert!(shares_combat_plane(CliffWalker, Air));
    assert!(shares_combat_plane(Ground, CliffWalker));
    assert!(!shares_combat_plane(Ground, Air));
    assert!(!shares_combat_plane(Air, CliffJumper));
}

/// B-79 (plan `2026-11-05-sc2-deviation-fixes`, found by WP2): a weapon's cooldown and
/// backswing run on every tick, also for a unit armed only against air whose target is gone.
/// They froze, because a unit without a target resolved its weapon against the ground plane,
/// found none, and skipped its weapon step.
#[test]
fn b79_an_air_only_weapon_keeps_cycling_after_its_target_is_gone() {
    use sim_types::MoverType::{Air, Ground};
    let mut arena = Arena::new(32);
    let slug = arena
        .world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == Air
                && a.weapon_against(Ground, None).is_none()
                && a.weapon_against(Air, None)
                    .is_some_and(|w| w.period_ticks > 4)
        })
        .map(|a| a.slug.clone())
        .expect("the shipped catalog has a unit armed only against air");
    let shooter = arena.spawn(&slug, BLUE, 10.0, 10.0);
    let target = arena.spawn(&slug, RED, 13.0, 10.0);
    arena.order(BLUE, &[shooter], attack(target));
    for _ in 0..100 {
        if arena.world.entities[&shooter].weapon_cooldown_remaining > 2 {
            break;
        }
        arena.run(1);
    }
    let before = arena.world.entities[&shooter].weapon_cooldown_remaining;
    assert!(before > 2, "{slug} never fired");
    // Rule fixture: another source kills the target, so the shooter has none left.
    arena.world.entities.get_mut(&target).unwrap().hp = SimScalar::ZERO;
    arena.run(2);
    assert_eq!(
        arena.world.entities[&shooter].weapon_cooldown_remaining,
        before - 2,
        "{slug}'s cooldown must keep running without a target"
    );
}
