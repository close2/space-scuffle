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

#![allow(clippy::expect_used, clippy::unwrap_used)]

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
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
            world: SimWorld::new(map, 7),
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
    UnitOrder::AttackTarget { target }
}

/// S-4: a projectile weapon's `attacks_count` strikes all land, as on the instant path.
#[test]
fn s4_projectile_weapon_applies_every_strike() {
    let mut arena = Arena::new(32);
    // Dual Gauss Pistols: a two-strike projectile weapon, at 4 + 5 vs light on each strike.
    let shooter = arena.spawn("jet_jumper", BLUE, 10.0, 10.0);
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
    let mode = arena.world.execute_ability(tank, 0, None, None);
    assert!(mode, "siege mode must start");
    arena.run(80);

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
