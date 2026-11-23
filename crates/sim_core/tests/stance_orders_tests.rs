//! # Integration Test: the plain orders of a unit's settled stance
//!
//! Plan `2026-11-23-anchored-commands` WP1 (D-1, D-2; acceptance P-2). Each mode in
//! `units.ron` authors which of Stop and Hold Position a unit in it accepts
//! (`UnitModeSpec::orders`, a [`StanceOrders`]); the archetype's own stance accepts both when
//! the archetype is mobile and neither when it is not. [`UnitArchetype::stance_orders`] is that
//! rule, and [`sim_core::entity::Entity::settled_stance_orders`] applies it to the stance a unit
//! is in or, while it channels a switch, the stance it will be in when the channel ends. The
//! client's command card and its admission both read it, so a button is shown exactly when
//! pressing it reaches a unit.
//!
//! Every test drives the shipped catalog through `SimWorld::spawn_unit`,
//! `SimWorld::execute_ability` and `SimWorld::step`. Archetypes and modes are found by property;
//! no slug or mode name is written here (charter C5).

#![allow(clippy::expect_used, clippy::panic)]

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, EntityId, PlayerId, SimScalar, SimVec2, StanceOrders, Tick, TickBundle,
};

/// Ticks a switch may take at most, its random extra included, before a test gives up.
const PATIENCE: u32 = 400;

/// A 48 × 48 flat world on the shipped catalog.
fn flat_world(seed: u64) -> SimWorld {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat 48x48 map is valid");
    sim_fixtures::world(map, seed)
}

/// Steps `world` one tick without commands.
fn step(world: &mut SimWorld) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands: Vec::new(),
        state_checksum: None,
    });
}

/// Every shipped archetype that declares a mode, with each mode's switch ability index.
fn moded_archetypes(world: &SimWorld) -> Vec<(UnitArchetype, Vec<(usize, u8)>)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| !arch.modes.is_empty())
        .map(|arch| {
            let switches = arch
                .modes
                .iter()
                .enumerate()
                .map(|(mode, spec)| {
                    let ability = arch
                        .abilities
                        .iter()
                        .position(|ability| {
                            matches!(&ability.effect,
                                AbilityEffectSpec::SwitchMode { mode_name } if *mode_name == spec.mode_name)
                        })
                        .expect("every shipped mode has a switch ability");
                    (mode, u8::try_from(ability).expect("few abilities"))
                })
                .collect();
            (arch.clone(), switches)
        })
        .collect()
}

/// The settled stance's plain orders of `unit`.
fn settled_orders(world: &SimWorld, unit: EntityId) -> StanceOrders {
    let entity = &world.entities[&unit];
    let archetype = world
        .catalog
        .get(entity.unit_type_id)
        .expect("the unit's archetype");
    entity.settled_stance_orders(archetype)
}

/// Steps `world` until `unit` no longer channels.
fn finish_channel(world: &mut SimWorld, unit: EntityId) {
    for _ in 0..PATIENCE {
        if world.entities[&unit].channeling_remaining_ticks == 0 {
            return;
        }
        step(world);
    }
    panic!("the switch ends within {PATIENCE} ticks");
}

#[test]
fn the_default_stance_accepts_both_orders_exactly_when_the_archetype_is_mobile() {
    let world = flat_world(0x5a_0001);
    let mut mobile = 0;
    let mut immobile = 0;
    for arch in world.catalog.archetypes() {
        let expected = if arch.is_immobile_by_default() {
            immobile += 1;
            StanceOrders::NONE
        } else {
            mobile += 1;
            StanceOrders::ALL
        };
        assert_eq!(arch.stance_orders(None), expected, "{}", arch.slug);
    }
    assert!(
        mobile > 0 && immobile > 0,
        "{mobile} mobile, {immobile} immobile"
    );
}

#[test]
fn a_mode_accepts_exactly_its_authored_orders() {
    let world = flat_world(0x5a_0002);
    let moded = moded_archetypes(&world);
    assert!(!moded.is_empty(), "the shipped catalog has a mode");
    for (arch, _) in &moded {
        for mode in &arch.modes {
            assert_eq!(
                arch.stance_orders(Some(mode)),
                mode.orders,
                "{} {}",
                arch.slug,
                mode.mode_name
            );
        }
    }
}

#[test]
fn a_unit_answers_for_the_stance_it_settles_in_through_both_channels() {
    for (arch, switches) in moded_archetypes(&flat_world(0x5a_0003)) {
        for (mode, ability) in switches {
            let mut world = flat_world(0x5a_0003);
            let unit = world
                .spawn_unit(
                    arch.unit_type_id,
                    PlayerId(0),
                    SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(20)),
                )
                .expect("spawn the moded unit");
            let label = format!("{} {}", arch.slug, arch.modes[mode].mode_name);
            let default = arch.stance_orders(None);
            let authored = arch.modes[mode].orders;
            assert_eq!(settled_orders(&world, unit), default, "{label}: default");

            assert!(world.execute_ability(unit, ability, None, None), "{label}");
            assert!(world.entities[&unit].channeling_remaining_ticks > 0);
            assert_eq!(world.entities[&unit].active_mode, None, "{label}");
            assert_eq!(
                settled_orders(&world, unit),
                authored,
                "{label}: entering answers for the mode"
            );
            finish_channel(&mut world, unit);
            assert_eq!(
                settled_orders(&world, unit),
                authored,
                "{label}: in the mode"
            );

            assert!(world.execute_ability(unit, ability, None, None), "{label}");
            assert!(world.entities[&unit].channeling_remaining_ticks > 0);
            assert!(world.entities[&unit].active_mode.is_some(), "{label}");
            assert_eq!(
                settled_orders(&world, unit),
                default,
                "{label}: leaving answers for the default stance"
            );
            finish_channel(&mut world, unit);
            assert_eq!(settled_orders(&world, unit), default, "{label}: back");
        }
    }
}

#[test]
fn the_shipped_modes_differ_from_their_default_stance_in_hold_position_only_where_authored() {
    // The shipped anchoring modes all keep Stop (5.0.14 `SiegeTankSieged` and
    // `LurkerMPBurrowed` carry `stop`), and at least one gives up Hold Position (the sieged
    // tank has no `move`): the rule is not the mobility rule.
    let world = flat_world(0x5a_0004);
    let anchoring: Vec<_> = world
        .catalog
        .archetypes()
        .flat_map(|arch| arch.modes.iter())
        .filter(|mode| mode.is_immobile)
        .collect();
    assert!(!anchoring.is_empty(), "a shipped mode anchors");
    assert!(anchoring.iter().all(|mode| mode.orders.stop));
    assert!(anchoring.iter().any(|mode| !mode.orders.hold_position));
    assert!(anchoring.iter().any(|mode| mode.orders.hold_position));
}
