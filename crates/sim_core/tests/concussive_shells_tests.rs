//! # Integration Test: the concussive slow (plan `2026-11-05-sc2-deviation-fixes` `WP13d`)
//!
//! The shipped Marauder analogue's research `concussive_shells`, played through `SimWorld::step`, against
//! the `StarCraft` II 5.0.14 data the plan's amendment cites: the weapon's impact set
//! `PunisherGrenadesSet` runs `PunisherGrenadesSlow` (liberty EffectData:1641-1660) only once
//! the owner has researched the upgrade (`PunisherGrenadesResearched`, ValidatorData:654-658),
//! never on a massive unit (`-;Massive`, :1424-1426) or a structure (`NotStructure`,
//! :1084-1086). It applies the buff `Slow` (liberty BehaviorData:1256-1262): movement × 0.5 for
//! 1.5 Normal seconds, 26.785714 ticks, so a unit slowed on tick `t` is free again on tick
//! `t + 27`; one per unit (`MaxStackCount` 1, core BehaviorData:43), so a later hit refreshes it.
//!
//! The tests read the shipped catalogs (`units.ron`, `upgrades.ron`, `ability_effects.ron`,
//! `statuses.ron`), so a data edit that breaks a rule fails here. Units are named by slug in
//! this test only (charter §3.3 governs production code).

#![allow(clippy::expect_used, clippy::panic)]

use sim_core::{SimCoreError, SimWorld, UnitCatalog, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilitySource, EffectContext, EffectTarget, EntityId, PlayerId, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UpgradeEffect,
};

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The Marauder analogue.
const GOLEM: &str = "shock_golem";
/// The research.
const RESEARCH: &str = "concussive_shells";
/// The status and the effect that applies it.
const SLOW: &str = "concussive_shells_slow";
/// Ticks after the application on which the slow is gone: 26.785714 elapsed, so the 27th.
const EXPIRY: u32 = 27;

fn world() -> SimWorld {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat map");
    sim_fixtures::world(map, 0x0C_0C55)
}

fn at(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// Spawns the shipped unit `slug` for `owner` at `(x, y)` holding position.
fn held(world: &mut SimWorld, slug: &str, owner: PlayerId, x: f32, y: f32) -> EntityId {
    let kind = world.catalog.id_by_slug(slug).expect("a shipped unit");
    let id = world
        .spawn_unit(kind, owner, at(x, y))
        .expect("invariant: a free spot");
    world
        .entities
        .get_mut(&id)
        .expect("spawned")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    id
}

/// Steps one tick; panics on a diagnostic.
fn step(world: &mut SimWorld) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands: Vec::new(),
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Whether `id` carries the shipped slow.
fn slowed(world: &SimWorld, id: EntityId) -> bool {
    let status = world
        .catalog
        .spells()
        .status_id(SLOW)
        .expect("a shipped status");
    world.effects().has_status(id, status)
}

/// A golem of blue at (20, 20) ordered onto `target`; steps until `target` first loses life and
/// returns whether it was slowed on that tick.
fn first_hit(world: &mut SimWorld, target: EntityId) -> bool {
    let golem = held(world, GOLEM, BLUE, 20.0, 20.0);
    world
        .entities
        .get_mut(&golem)
        .expect("spawned")
        .issue_order(
            UnitOrder::AttackTarget {
                target,
                origin: sim_types::AttackOrigin::default(),
            },
            false,
        )
        .expect("attack admitted");
    let life = world.entities[&target].hp;
    for _ in 0..120 {
        step(world);
        if world.entities[&target].hp < life {
            return slowed(world, target);
        }
    }
    panic!("the golem hits within 120 ticks");
}

/// Applies the shipped slow effect to `id` directly, as a hit does.
fn apply_slow(world: &mut SimWorld, id: EntityId) {
    let effect = world
        .catalog
        .spells()
        .effect_id(SLOW)
        .expect("a shipped effect");
    let position = world.entities[&id].position;
    world.run_effect(
        effect,
        EffectContext {
            owner: BLUE,
            caster: None,
            ability: AbilitySource::default(),
            origin: position,
            source_point: position,
            target: EffectTarget::Unit(id),
        },
    );
}

#[test]
fn the_research_unlocks_the_slow_through_catalog_data() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let upgrade = catalogs.upgrades.get_by_slug(RESEARCH).expect("shipped");
    assert_eq!(
        upgrade.effects,
        vec![UpgradeEffect::WeaponHitEffect {
            target_slug: GOLEM.to_owned(),
            effect: SLOW.to_owned(),
        }]
    );
    let unlocks = catalogs.upgrades.weapon_hit_effects(GOLEM);
    assert_eq!(unlocks.len(), 1);
    assert_eq!(unlocks[0].upgrade_slug, RESEARCH);
    let status = catalogs
        .units
        .spells()
        .status(catalogs.units.spells().status_id(SLOW).expect("shipped"))
        .expect("shipped");
    // Slow, liberty BehaviorData:1260-1261: 1.5 Normal s × 25 ÷ 1.4 ticks, movement × 0.5.
    assert_eq!(status.spec.duration_ticks, SimScalar::from_num(26.785_714));
    assert_eq!(status.spec.move_speed_factor, SimScalar::HALF);
}

#[test]
fn without_the_research_a_hit_does_not_slow() {
    let mut world = world();
    let target = held(&mut world, "pulse_bot", RED, 24.0, 20.0);
    assert!(!first_hit(&mut world, target));
    assert_eq!(
        world.entities[&target].status_modifiers().move_speed_factor,
        SimScalar::ONE
    );
}

#[test]
fn with_the_research_each_hit_halves_movement_speed() {
    let mut world = world();
    world
        .research_upgrade(BLUE, RESEARCH)
        .expect("research admitted");
    let target = held(&mut world, "pulse_bot", RED, 24.0, 20.0);
    assert!(first_hit(&mut world, target), "the first hit slows");
    assert_eq!(
        world.entities[&target].status_modifiers().move_speed_factor,
        SimScalar::HALF
    );
}

#[test]
fn the_research_of_another_player_does_not_slow() {
    let mut world = world();
    world
        .research_upgrade(RED, RESEARCH)
        .expect("research admitted");
    let target = held(&mut world, "pulse_bot", RED, 24.0, 20.0);
    assert!(!first_hit(&mut world, target));
}

#[test]
fn massive_units_and_structures_are_never_slowed() {
    let mut world = world();
    world
        .research_upgrade(BLUE, RESEARCH)
        .expect("research admitted");
    let massive = held(&mut world, "colossus_mech", RED, 24.5, 20.0);
    assert!(!first_hit(&mut world, massive), "a massive unit is immune");

    let mut world = self::world();
    world
        .research_upgrade(BLUE, RESEARCH)
        .expect("research admitted");
    let kind = world
        .catalog
        .id_by_slug("tech_module_pod")
        .expect("a shipped structure");
    let structure = world
        .spawn_structure(kind, RED, 23, 19)
        .expect("a free spot");
    assert!(!first_hit(&mut world, structure), "a structure is immune");
}

#[test]
fn the_slow_lasts_27_ticks() {
    let mut world = world();
    let target = held(&mut world, "pulse_bot", RED, 40.0, 40.0);
    apply_slow(&mut world, target);
    assert!(slowed(&world, target));
    for elapsed in 1..EXPIRY {
        step(&mut world);
        assert!(
            slowed(&world, target),
            "slowed {elapsed} ticks after the hit"
        );
    }
    step(&mut world);
    assert!(!slowed(&world, target), "free {EXPIRY} ticks after the hit");
    assert_eq!(
        world.entities[&target].status_modifiers().move_speed_factor,
        SimScalar::ONE
    );
}

#[test]
fn a_later_hit_refreshes_the_slow_and_does_not_stack() {
    let mut world = world();
    let target = held(&mut world, "pulse_bot", RED, 40.0, 40.0);
    apply_slow(&mut world, target);
    for _ in 0..20 {
        step(&mut world);
    }
    apply_slow(&mut world, target);
    assert_eq!(world.effects().statuses_of(target).len(), 1, "one slow");
    assert_eq!(
        world.entities[&target].status_modifiers().move_speed_factor,
        SimScalar::HALF,
        "not 0.25"
    );
    // Refreshed at elapsed 20: free 27 ticks after the second hit, not the first.
    for elapsed in 1..EXPIRY {
        step(&mut world);
        assert!(
            slowed(&world, target),
            "slowed {elapsed} ticks after the refresh"
        );
    }
    step(&mut world);
    assert!(!slowed(&world, target));
}

#[test]
fn a_weapon_hit_effect_must_name_a_shipped_effect_and_an_armed_target() {
    let factions = sim_fixtures::shipped_factions();
    let edited = |from: &str, to: &str| {
        assert!(sim_fixtures::UPGRADES_RON.contains(from));
        UpgradeCatalog::from_ron_str(&factions, &sim_fixtures::UPGRADES_RON.replace(from, to))
            .expect("parses")
    };
    let units: UnitCatalog = sim_fixtures::shipped_catalogs().units;
    let unknown = edited(
        "effect: \"concussive_shells_slow\"",
        "effect: \"no_such_effect\"",
    );
    assert!(matches!(
        units.validate_weapon_hit_effects(&unknown),
        Err(SimCoreError::CatalogInvalidValue { field, .. }) if field == "WeaponHitEffect.effect"
    ));
    let unarmed = edited(
        "target_slug: \"shock_golem\",\n                    effect",
        "target_slug: \"tech_module_pod\",\n                    effect",
    );
    assert!(matches!(
        units.validate_upgrade_references(&unarmed),
        Err(SimCoreError::CatalogInvalidValue { field, .. })
            if field == "WeaponHitEffect.target_slug"
    ));
}
